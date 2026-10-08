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

/// Compares the number groups, so `2026-10-08-1415` beats `2026-10-07-0900`
/// and also beats `2026-10-08-0900`. A plain string compare gets that wrong
/// and would never offer the update.
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

/// Shows the stamp the way people write dates, `08-10-2026 14:36`.
///
/// The stored stamp stays `2026-10-08-1436` on purpose. Day first does not
/// survive a numeric compare of the parts, and the file name carries the stamp,
/// so reordering it would leave every build that is already installed unable to
/// tell that a newer one exists. Only the display is reordered.
pub fn pretty(version: &str) -> String {
    let parts: Vec<&str> = version.split('-').collect();
    if parts.len() != 4 || parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
    {
        return version.to_string();
    }
    if parts[3].len() != 4 {
        return version.to_string();
    }
    format!(
        "{}-{}-{} {}:{}",
        parts[2],
        parts[1],
        parts[0],
        &parts[3][..2],
        &parts[3][2..]
    )
}

/// Pulls the version out of `Cheesestrap-Setup-2026-10-08-1415-x64.exe`. The
fn header_num(res: &reqwest::blocking::Response, name: &str) -> Option<u64> {
    res.headers().get(name)?.to_str().ok()?.parse().ok()
}

/// tag itself is always just "release", so the asset name is what carries it.
fn version_from_asset(name: &str) -> Option<String> {
    let rest = name.strip_prefix("Cheesestrap-Setup-")?;
    let rest = rest.strip_suffix(".exe")?;
    let rest = rest.strip_suffix("-x64").unwrap_or(rest);
    let parts: Vec<&str> = rest.split('-').collect();
    let looks_stamped = parts.len() == 4
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if !looks_stamped {
        return None;
    }
    Some(rest.to_string())
}

/// `Ok(None)` means we are already on the newest release.
///
/// The rate limit is unauthenticated, 60 requests an hour for the whole ip
/// address, so it is worth reading the headers back out. A silent 403 that we
/// treat as "no update" is how a version check quietly dies.
pub fn check(tx: &Sender<WorkerMsg>) -> Result<Option<Release>, String> {
    let res = crate::net::client()
        .get(API)
        .header("accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("could not reach github: {e}"))?;

    let remaining = header_num(&res, "x-ratelimit-remaining");
    let reset = header_num(&res, "x-ratelimit-reset").unwrap_or(0);

    if !res.status().is_success() {
        if res.status().as_u16() == 403 && remaining == Some(0) {
            let mins = reset.saturating_sub(crate::app::unix_now()) / 60 + 1;
            let msg = format!("github rate limit reached, asks again in {mins} min.");
            tx.send(WorkerMsg::Log(msg.clone())).ok();
            return Err(msg);
        }
        return Err(format!("version check failed: {}", res.status()));
    }
    if let Some(left) = remaining {
        if left <= 10 {
            tx.send(WorkerMsg::Log(format!(
                "github rate limit is down to {left} of 60 for this ip."
            )))
            .ok();
        }
    }

    let body = res.text().map_err(|e| format!("version check failed: {e}"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_builds_win() {
        assert!(newer("2026-10-08-1415", "2026-10-07-2359"));
        assert!(newer("2026-10-08-1415", "2026-10-08-0900"));
        assert!(newer("2026-11-01-0000", "2026-10-31-2359"));
        assert!(newer("2027-01-01-0000", "2026-12-31-2359"));
        // same minute is not newer
        assert!(!newer("2026-10-08-1415", "2026-10-08-1415"));
        assert!(!newer("2026-10-08-0900", "2026-10-08-1415"));
        // the day has to win over the time, this is the trap
        assert!(newer("2026-10-09-0001", "2026-10-08-2359"));
        // the crate version a plain cargo build falls back to still compares
        assert!(newer("2026-10-08-1415", "1.0.0"));
    }

    // Guards the reason the stamp stays iso. If it were day first, a build
    // that is already installed would read the newest file name as
    // 10 < 2026 and decide it was never offered an update.
    #[test]
    fn a_day_first_stamp_would_be_invisible_to_installed_builds() {
        let installed = "2026-10-08-1438";
        let day_first = "08-10-2026-1445"; // same day, seven minutes later
        assert!(!newer(day_first, installed), "day first cannot be shipped");
        assert!(newer("2026-10-08-1445", installed), "iso can");
    }

    #[test]
    fn stamps_display_nicely() {
        // day first on screen, iso in the file name
        assert_eq!(pretty("2026-10-08-1415"), "08-10-2026 14:15");
        assert_eq!(pretty("2026-01-02-0304"), "02-01-2026 03:04");
        // anything that is not a stamp is left alone
        assert_eq!(pretty("1.0.0"), "1.0.0");
        assert_eq!(pretty(""), "");
    }

    #[test]
    fn asset_name_carries_the_stamp() {
        assert_eq!(
            version_from_asset("Cheesestrap-Setup-2026-10-08-1415-x64.exe").as_deref(),
            Some("2026-10-08-1415")
        );
        // the old hand written scheme is not a stamp, skip it rather than
        // treating "1.0.10" as a version from the year 1
        assert!(version_from_asset("Cheesestrap-Setup-1.0.10-x64.exe").is_none());
        assert!(version_from_asset("cheesestrap notes.txt").is_none());
        assert!(version_from_asset("Cheesestrap-Setup-x64.exe").is_none());
    }
}