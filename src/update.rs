//! Self update: ask the github releases api whether a newer cheesestrap is
//! out, pull its setup exe, and hand the install over to it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;

use crate::app::WorkerMsg;

const API: &str = "https://api.github.com/repos/nenquen/cheesestrap/releases?per_page=10";

/// The running version.
///
/// Normally the date based stamp that `installer\stamp-version.ps1` puts in the
/// environment, so it is when this binary was built. Falls back to the crate
/// version for a plain `cargo build` with no stamp, which is only ever a local
/// dev build.
pub fn current() -> &'static str {
    option_env!("CHEESESTRAP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// The self update replaces our own exe, which only works from the installed
/// copy. A loose exe sitting in a downloads folder would get the setup run on
/// it and end up with a second copy under program files while the one the user
/// launched stayed old, so we just skip instead.
pub fn can_self_update() -> bool {
    let Some(dir) = crate::roblox::app_base_dir() else {
        return false;
    };
    dir.join("unins000.exe").exists() || dir.join("unins000.dat").exists()
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Release {
    pub version: String,
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

/// `Ok(None)` means we are already on the newest release.
pub fn check() -> Result<Option<Release>, String> {
    let body = crate::net::client()
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
    let releases = v
        .as_array()
        .ok_or("release list was not an array.".to_string())?;

    // Not /releases/latest on purpose. github decides "latest" by created_at,
    // and we keep amending the same release instead of making a new one, so its
    // created_at never moves. The day a second release exists the endpoint can
    // hand back the old one. Taking the highest version across everything we
    // see does not care about ordering at all.
    let mut best: Option<Release> = None;
    for r in releases {
        let Some(rel) = parse_release(r) else {
            continue;
        };
        let better = best
            .as_ref()
            .is_none_or(|b| newer(&rel.version, &b.version));
        if better {
            best = Some(rel);
        }
    }

    let best = best.ok_or("no release with an installable exe.".to_string())?;
    if !newer(&best.version, current()) {
        return Ok(None);
    }
    Ok(Some(best))
}

/// Pulls the setup asset and the version out of one release object.
fn parse_release(release: &serde_json::Value) -> Option<Release> {
    // drafts and prereleases are things being worked on, not things to ship
    if release["draft"].as_bool() == Some(true)
        || release["prerelease"].as_bool() == Some(true)
    {
        return None;
    }
    let assets = release["assets"].as_array()?;
    // prefer the setup exe, it is the thing we can actually install
    let pick = assets
        .iter()
        .find(|a| {
            a["name"]
                .as_str()
                .is_some_and(|n| n.starts_with("Cheesestrap-Setup-"))
        })
        .or_else(|| {
            assets
                .iter()
                .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(".exe")))
        })?;
    let asset_name = pick["name"].as_str()?.to_string();
    let asset_url = pick["browser_download_url"].as_str()?.to_string();
    if asset_url.is_empty() {
        return None;
    }
    Some(Release {
        version: version_from_asset(&asset_name)?,
        asset_name,
        asset_url,
        size: pick["size"].as_u64().unwrap_or(0),
    })
}

/// Streams the asset to the temp folder, reporting progress the same way the
/// roblox download does so the bar behaves identically for both.
pub fn download(release: &Release, tx: &Sender<WorkerMsg>) -> Result<PathBuf, String> {
    tx.send(WorkerMsg::Log(format!(
        "downloading cheesestrap {} ({} MB)...",
        release.version,
        release.size / 1_048_576
    )))
    .ok();

    let tmp = std::env::temp_dir().join(&release.asset_name);
    crate::net::download_to_checked(&release.asset_url, &tmp, tx, release.size)?;
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
    // inno re-launches itself out of a temp folder, so this copy is free to
    // delete once the setup is done with it
    let script = format!(
        "Wait-Process -Id {pid} -ErrorAction SilentlyContinue; \
         $p = Start-Process -FilePath '{}' -Verb RunAs -PassThru \
         -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART'; \
         $p.WaitForExit(); \
         Remove-Item '{}' -Force -ErrorAction SilentlyContinue",
        setup.display(),
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