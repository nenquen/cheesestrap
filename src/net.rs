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

pub const USER_AGENT: &str = concat!("Cheesestrap/", env!("CARGO_PKG_VERSION"));

pub fn client() -> &'static reqwest::blocking::Client {
    static C: std::sync::OnceLock<reqwest::blocking::Client> =
        std::sync::OnceLock::new();
    C.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
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