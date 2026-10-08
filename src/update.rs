//! Self update: ask the github releases api whether a newer cheesestrap is
//! out, pull its setup exe, and hand the install over to it.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;

use crate::app::WorkerMsg;

const API: &str = "https://api.github.com/repos/nenquen/cheesestrap/releases/latest";

/// The one place the version is read from, so cargo stays the single source.
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Clone)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub asset_name: String,
    pub asset_url: String,
    pub size: u64,
}

/// Numeric dotted compare, so 1.0.10 beats 1.0.9. A plain string compare
/// gets that wrong and would never offer the update.
pub fn newer(candidate: &str, base: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split(['.', '-'])
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parse(candidate), parse(base));
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Pulls the version out of `Cheesestrap-Setup-1.2.3-x64.exe`. The tag itself
/// is always just "release", so the asset name is what carries the number.
fn version_from_asset(name: &str) -> Option<String> {
    let rest = name.strip_prefix("Cheesestrap-Setup-")?;
    let rest = rest.strip_suffix(".exe")?;
    let rest = rest.strip_suffix("-x64").unwrap_or(rest);
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn http() -> &'static reqwest::blocking::Client {
    static C: std::sync::OnceLock<reqwest::blocking::Client> =
        std::sync::OnceLock::new();
    C.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(concat!("Cheesestrap/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("http client")
    })
}

/// `Ok(None)` means we are already on the newest release.
pub fn check() -> Result<Option<Release>, String> {
    let body = http()
        .get(API)
        .header("accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("could not reach github: {e}"))?
        .error_for_status()
        .map_err(|e| format!("version check failed: {e}"))?
        .text()
        .map_err(|e| format!("version check failed: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("bad release json: {e}"))?;

    let assets = v["assets"].as_array().cloned().unwrap_or_default();
    // prefer the setup exe, it is the thing we can actually install
    let mut pick = assets
        .iter()
        .find(|a| a["name"].as_str().is_some_and(|n| n.starts_with("Cheesestrap-Setup-")));
    if pick.is_none() {
        pick = assets.iter().find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(".exe")));
    }
    let Some(asset) = pick else {
        return Err("the release has no exe to install.".to_string());
    };
    let asset_name = asset["name"].as_str().unwrap_or_default().to_string();
    let asset_url = asset["browser_download_url"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if asset_url.is_empty() {
        return Err("the release asset has no download url.".to_string());
    }
    let size = asset["size"].as_u64().unwrap_or(0);
    let notes = v["body"].as_str().unwrap_or_default().trim().to_string();
    let version = match version_from_asset(&asset_name) {
        Some(v) => v,
        None => return Err("the release asset name has no version in it.".to_string()),
    };

    if !newer(&version, current()) {
        return Ok(None);
    }
    Ok(Some(Release {
        version,
        notes,
        asset_name,
        asset_url,
        size,
    }))
}

/// Streams the asset to the temp folder, reporting progress like the roblox
/// download does so the log looks the same for both.
pub fn download(release: &Release, tx: &Sender<WorkerMsg>) -> Result<PathBuf, String> {
    tx.send(WorkerMsg::Log(format!(
        "downloading cheesestrap {} ({} MB)...",
        release.version,
        release.size / 1_048_576
    )))
    .ok();

    let mut res = http()
        .get(&release.asset_url)
        .send()
        .map_err(|e| format!("download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download failed: {e}"))?;

    let total = res.content_length().unwrap_or(release.size);
    let tmp = std::env::temp_dir().join(&release.asset_name);
    let mut file = fs::File::create(&tmp).map_err(|e| format!("could not write: {e}"))?;
    let mut buf = vec![0u8; 128 * 1024];
    let mut done = 0u64;
    let mut last_pct = 0u8;
    loop {
        let n = res
            .read(&mut buf)
            .map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("could not write: {e}"))?;
        done += n as u64;
        if total > 0 {
            let pct = (done * 100 / total) as u8;
            if pct / 10 > last_pct / 10 {
                last_pct = pct;
                tx.send(WorkerMsg::Progress { done, total }).ok();
            }
        }
    }
    file.flush().ok();
    drop(file);
    Ok(tmp)
}

/// Runs the setup after we are out of the way.
///
/// The setup cannot overwrite a running exe, so a detached powershell waits for
/// our pid to go away and then starts it. Spawning it before we close keeps the
/// window on screen just long enough to show what happened.
///
/// `-Verb RunAs` is deliberate: the app normally already runs elevated, but if
/// that relaunch ever fails the setup would otherwise die on a consent prompt
/// nobody sees, leaving the user with no update and no clue.
pub fn spawn_setup(setup: &Path) -> Result<(), String> {
    let pid = std::process::id();
    let script = format!(
        "Wait-Process -Id {pid} -ErrorAction SilentlyContinue; \
         Start-Process -FilePath '{}' -Verb RunAs \
         -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART'",
        setup.display()
    );
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the setup: {e}"))
}