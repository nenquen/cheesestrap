//! The one http client and the one file downloader for the whole app.
//!
//! These used to live twice, once in roblox.rs and once in update.rs. Two
//! identical clients meant two connection pools and two user agent strings to
//! keep in step, and the two download loops drifted apart as they were edited.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::Sender;

use crate::app::WorkerMsg;

pub fn user_agent() -> String {
    format!("Cheesestrap/{}", crate::update::current())
}

pub fn client() -> &'static reqwest::blocking::Client {
    static C: std::sync::OnceLock<reqwest::blocking::Client> =
        std::sync::OnceLock::new();
    C.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(user_agent())
            // a total timeout would cut off the multi hundred megabyte roblox
            // download on a slow line, but a connect timeout only covers the
            // part that can hang forever on a bad network
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("http client")
    })
}

/// Streams `url` into `dest` and reports progress as `base + done` of `total`,
/// so several packages can share one bar. Progress is throttled to whole
/// percent steps, a per chunk message would flood the channel on a fast
/// connection and the ui cannot use that many anyway.
pub fn download_to(
    url: &str,
    dest: &Path,
    tx: &Sender<WorkerMsg>,
    base: u64,
    total: u64,
) -> Result<(), String> {
    let mut res = client()
        .get(url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    let mut done: u64 = 0;
    let mut last_pct: Option<u8> = None;
    loop {
        let n = res.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        // a response without a content length gives us no bar to fill
        if total == 0 {
            continue;
        }
        let pct = (done * 100 / total) as u8;
        if pct != 100 && last_pct == Some(pct) {
            continue;
        }
        last_pct = Some(pct);
        let _ = tx.send(WorkerMsg::Progress {
            done: base + done,
            total,
        });
    }
    Ok(())
}

/// Same as [`download_to`] but refuses to hand back a short file.
///
/// A transfer that dies halfway still returns success from the read loop, and
/// running a truncated setup is worse than not updating at all. `expect` is the
/// size github advertised, zero means it did not tell us.
pub fn download_to_checked(
    url: &str,
    dest: &Path,
    tx: &Sender<WorkerMsg>,
    expect: u64,
) -> Result<(), String> {
    if let Err(e) = download_to(url, dest, tx, 0, expect) {
        // a half written setup in temp is junk nobody will ever use
        let _ = fs::remove_file(dest);
        return Err(e);
    }
    let got = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    if got == 0 || (expect > 0 && got != expect) {
        let _ = fs::remove_file(dest);
        return Err(format!("download was {got} bytes, expected {expect}."));
    }
    Ok(())
}