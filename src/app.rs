use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use crate::roblox;

#[derive(Clone, Copy, PartialEq)]
pub enum Focus {
    Menu,
    Content,
}

pub enum WorkerMsg {
    Log(String),
    Versions {
        installed: Option<String>,
        latest: Option<String>,
    },
    Progress {
        done: u64,
        total: u64,
    },
    InstallDone(Result<String, String>),
    Launched(Result<String, String>),
    Uninstalled(Vec<String>),
    WebviewFixed(Vec<String>),
    /// `Ok(None)` means already up to date.
    UpdateCheck(Result<Option<crate::update::Release>, String>),
    UpdateDownloaded(Result<std::path::PathBuf, String>),
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SavedSettings {
    #[serde(default)]
    logs_to_file: bool,
    #[serde(default = "default_true")]
    show_hints: bool,
    /// unix seconds of the last successful github check
    #[serde(default)]
    checked_at: u64,
    /// which build ran that check. an app that was just installed has a cache
    /// written by the version it replaced, so this forces a fresh look.
    #[serde(default)]
    checked_for: String,
    /// what that check found, kept so an update still shows while offline or
    /// while github is rate limiting us
    #[serde(default)]
    seen: Option<crate::update::Release>,
}

fn default_true() -> bool {
    true
}

/// How long a github answer stays good.
///
/// GitHub allows 60 requests an hour per ip, so this has to exist at all, but
/// it must stay short. Six hours meant a release could sit invisible all
/// afternoon, and worse, a user whose cache was fresh could not see the very
/// update that fixed whatever the cache was hiding. Half an hour is two
/// requests an hour per user, which leaves plenty of headroom on the limit.
const CHECK_INTERVAL: u64 = 30 * 60;

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn settings_path() -> Option<std::path::PathBuf> {
    crate::roblox::app_base_dir().map(|d| d.join("settings.json"))
}

fn load_saved() -> Option<SavedSettings> {
    let p = settings_path()?;
    let text = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_settings(app: &App) {
    let Some(p) = settings_path() else {
        return;
    };
    if std::fs::create_dir_all(p.parent().unwrap()).is_err() {
        return;
    }
    let s = SavedSettings {
        logs_to_file: app.logs_to_file,
        show_hints: app.show_hints,
        checked_at: app.checked_at,
        checked_for: app.checked_for.clone(),
        seen: app.seen.clone(),
    };
    if let Ok(text) = serde_json::to_string_pretty(&s) {
        let _ = std::fs::write(p, text);
    }
}

pub struct App {
    pub focus: Focus,
    pub using_keyboard: bool,
    pub logged_pct: u64,
    pub menu_idx: usize,
    pub action_idx: usize,
    pub settings_idx: usize,
    pub logs_to_file: bool,
    pub show_hints: bool,
    pub close_at: Option<std::time::Instant>,
    pub can_uninstall: bool,
    pub webview_ok: bool,
    pub log_view_end: Option<usize>,
    pub hover: Option<(u8, usize)>,
    pub protocol_url: Option<String>,
    pub extra_args: String,
    pub editing_args: bool,
    pub installed: Option<String>,
    pub latest: Option<String>,
    pub log: Vec<String>,
    pub busy: bool,
    pub ctx_close_requested: bool,
    /// A newer cheesestrap on github, waiting for a decision.
    pub update: Option<crate::update::Release>,
    /// Set once the user said yes, drives the updating overlay.
    pub updating: bool,
    pub update_pct: u8,
    /// The notice box in grid cells, so egui can drop the logo into it.
    pub notice_rect: Option<(u16, u16, u16, u16)>,
    /// Update check cache, see `check_update` for why this exists.
    pub checked_at: u64,
    pub checked_for: String,
    pub seen: Option<crate::update::Release>,
    pub tx: Sender<WorkerMsg>,
    pub rx: Receiver<WorkerMsg>,
}

impl App {
    pub fn new(tx: Sender<WorkerMsg>, rx: Receiver<WorkerMsg>) -> Self {
        let saved = load_saved();
        // read everything off it before the struct literal moves it
        let logs_to_file = saved.as_ref().map(|s| s.logs_to_file).unwrap_or(false);
        let show_hints = saved.as_ref().map(|s| s.show_hints).unwrap_or(true);
        let checked_at = saved.as_ref().map(|s| s.checked_at).unwrap_or(0);
        let checked_for = saved.as_ref().map(|s| s.checked_for.clone()).unwrap_or_default();
        let seen = saved.and_then(|s| s.seen);
        Self {
            focus: Focus::Menu,
            using_keyboard: false,
            logged_pct: 0,
            menu_idx: 0,
            action_idx: 0,
            settings_idx: 0,
            logs_to_file,
            show_hints,
            close_at: None,
            can_uninstall: roblox::has_clients(),
            webview_ok: matches!(roblox::webview2_state(), roblox::Webview2State::Ok(_)),
            log_view_end: None,
            hover: None,
            protocol_url: None,
            extra_args: String::new(),
            editing_args: false,
            installed: None,
            latest: None,
            log: vec![
                "welcome to cheesestrap.".to_string(),
                App::webview2_summary(),
            ],
            busy: false,
            ctx_close_requested: false,
            update: None,
            updating: false,
            update_pct: 0,
            notice_rect: None,
            checked_at,
            checked_for,
            seen,
            tx,
            rx,
        }
    }

    pub fn show_settings(&self) -> bool {
        self.menu_idx == 1
    }

    pub fn save(&self) {
        save_settings(self);
    }

    pub fn refresh_flags(&mut self) {
        self.can_uninstall = roblox::has_clients();
        self.webview_ok = matches!(roblox::webview2_state(), roblox::Webview2State::Ok(_));
    }

    pub fn uninstall_roblox(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(WorkerMsg::Log("uninstalling everything roblox...".to_string()));
            match roblox::uninstall() {
                Ok(lines) => {
                    let _ = tx.send(WorkerMsg::Uninstalled(lines));
                }
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Uninstalled(vec![format!(
                        "uninstall failed: {e}"
                    )]));
                }
            }
        });
    }

    /// Kills the stale edge update keys that point at a folder edge deleted,
    /// so a reinstall is not mistaken for an up to date runtime.
    fn clear_stale_webview_keys() -> usize {
        use winreg::RegKey;
        let mut n = 0;
        for (hive, sub) in roblox::wv2_client_keys() {
            let hk = RegKey::predef(hive);
            // only drop it when the folder it names is really gone, never a
            // working install
            let location = hk
                .open_subkey(&sub)
                .ok()
                .and_then(|k| k.get_value::<String, _>("location").ok())
                .unwrap_or_default();
            if location.is_empty() {
                continue;
            }
            if std::path::Path::new(&location)
                .join("msedgewebview2.exe")
                .exists()
            {
                continue;
            }
            if hk.delete_subkey_all(&sub).is_ok() {
                n += 1;
            }
        }
        n
    }

    /// Downloads and installs the webview2 runtime, after wiping any stale
    /// registration that would make roblox think it is already there.
    pub fn repair_webview2(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            match Self::clear_stale_webview_keys() {
                0 => {
                    let _ = tx.send(WorkerMsg::Log(
                        "no stale webview2 registration found.".to_string(),
                    ));
                }
                n => {
                    let _ = tx.send(WorkerMsg::Log(format!(
                        "cleared {n} stale webview2 registration(s)."
                    )));
                }
            }
            match roblox::install_webview2_runtime(&tx) {
                Ok(lines) => {
                    let _ = tx.send(WorkerMsg::WebviewFixed(lines));
                }
                Err(e) => {
                    let _ = tx.send(WorkerMsg::WebviewFixed(vec![format!(
                        "repair failed: {e}"
                    )]));
                }
            }
        });
    }

    /// One line describing the runtime, for the settings row and the log.
    pub fn webview2_summary() -> String {
        match roblox::webview2_state() {
            roblox::Webview2State::Ok(v) if v.is_empty() => "webview2 runtime: installed".to_string(),
            roblox::Webview2State::Ok(v) => format!("webview2 runtime: {v}"),
            roblox::Webview2State::Missing => {
                "webview2 runtime: not installed, roblox will ask for it".to_string()
            }
            roblox::Webview2State::Broken {
                version,
                location,
            } => {
                let v = if version.is_empty() {
                    "unknown".to_string()
                } else {
                    version
                };
                let l = if location.is_empty() {
                    "an empty path".to_string()
                } else {
                    location
                };
                format!("webview2 runtime: broken, {v} registered but {l} is gone")
            }
        }
    }

    /// Asks github whether a newer release is out. Runs on its own thread and
    /// never blocks the ui, a slow or blocked api just means no popup.
    pub fn check_update(&mut self) {
        if self.updating {
            return;
        }
        if !crate::update::can_self_update() {
            self.push_log(
                "not an installed copy, skipping the cheesestrap update check.".to_string(),
            );
            return;
        }
        // Only github knows about a new release, but asking it costs one of 60
        // requests per hour per ip address. Checking on every launch meant a
        // handful of restarts, or two people behind the same router, burned
        // that quota and then every user silently stopped getting updates
        // because the api started answering 403.
        //
        // So the answer is cached on disk. A cached update is still shown, so
        // being offline or throttled never hides an update that is waiting.
        let now = unix_now();
        // A cache written by a different build is not ours to trust. Right
        // after an update install the old settings file is still on disk, and
        // honouring its timestamp meant a fresh install could sit on "up to
        // date" without ever asking, which is exactly when an update is most
        // likely waiting.
        let mine = self.checked_for == crate::update::current();
        let fresh = mine && now.saturating_sub(self.checked_at) < CHECK_INTERVAL;
        let pending = self
            .seen
            .clone()
            .filter(|s| crate::update::newer(&s.version, crate::update::current()));

        if fresh {
            // the cached answer still stands, so answer from it and skip the
            // api entirely rather than showing the same thing twice
            match pending {
                Some(rel) => self.offer_update(rel),
                None => self.push_log("cheesestrap is up to date.".to_string()),
            }
            return;
        }
        // stale cache. ask again. if we already know about an update the
        // screen stays up in the meantime and a failure costs nothing.
        let tx = self.tx.clone();
        thread::spawn(move || {
            let r = crate::update::check(&tx);
            let _ = tx.send(WorkerMsg::UpdateCheck(r));
        });
    }

    fn offer_update(&mut self, rel: crate::update::Release) {
        self.push_log(format!(
            "cheesestrap {} is available, you have {}.",
            crate::update::pretty(&rel.version),
            crate::update::pretty(crate::update::current())
        ));
        self.update = Some(rel);
    }

    /// The user pressed update: pull the setup and hand the install over.
    pub fn apply_update(&mut self) {
        if self.updating {
            return;
        }
        let Some(release) = self.update.clone() else {
            return;
        };
        self.updating = true;
        self.update_pct = 0;
        self.push_log(format!(
            "updating to cheesestrap {}...",
            release.version
        ));
        let tx = self.tx.clone();
        thread::spawn(move || {
            let r = crate::update::download(&release, &tx);
            let _ = tx.send(WorkerMsg::UpdateDownloaded(r));
        });
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        let line = line.into();
        self.log.push(line.clone());
        if self.log.len() > 500 {
            let n = self.log.len() - 500;
            self.log.drain(..n);
            if let Some(end) = self.log_view_end.as_mut() {
                *end = end.saturating_sub(n);
            }
        }
        if self.logs_to_file {
            append_log_file(&line);
        }
    }

    pub fn scroll_log(&mut self, up_lines: i32) {
        let len = self.log.len() as i32;
        let mut end = self.log_view_end.unwrap_or(self.log.len()) as i32;
        end -= up_lines;
        if end >= len {
            self.log_view_end = None;
        } else {
            self.log_view_end = Some(end.max(0) as usize);
        }
    }

    pub fn refresh_versions(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let installed = roblox::latest_installed();
            let latest = roblox::fetch_latest_guid()
                .map_err(|e| e.to_string())
                .ok();
            let _ = tx.send(WorkerMsg::Versions { installed, latest });
        });
    }

    pub fn play_label(&self) -> &'static str {
        match (&self.installed, &self.latest) {
            (Some(a), Some(b)) if a == b => {
                if roblox::client_ok(a) {
                    "play"
                } else {
                    "update"
                }
            }
            (None, Some(_)) => "install",
            (Some(_), Some(_)) => "update",
            _ => "play",
        }
    }

    pub fn play(&mut self) {
        self.run(None);
    }

    pub fn play_with_url(&mut self, url: String) {
        self.run(Some(url));
    }

    fn run(&mut self, join_url: Option<String>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.logged_pct = 0;
        let tx = self.tx.clone();
        let extra_args = self.extra_args.clone();
        let installed = self.installed.clone();
        thread::spawn(move || {
            let _ = tx.send(WorkerMsg::Log("checking for updates...".to_string()));
            let latest = match roblox::fetch_latest_guid() {
                Ok(g) => g,
                Err(e) => {
                    let _ = tx.send(WorkerMsg::InstallDone(Err(format!(
                        "version check failed: {e}"
                    ))));
                    return;
                }
            };
            if installed.as_deref() != Some(latest.as_str())
                || !roblox::client_ok(&latest)
            {
                let _ = tx.send(WorkerMsg::Log(format!("updating to {latest}...")));
                match roblox::download_client(&latest, &tx) {
                    Ok(()) => {
                        let _ = tx.send(WorkerMsg::InstallDone(Ok(latest.clone())));
                    }
                    Err(e) => {
                        let _ = tx.send(WorkerMsg::InstallDone(Err(e)));
                    }
                }
                return;
            }
            if let Err(e) = roblox::ensure_loader_dll(&latest, &tx) {
                let _ = tx.send(WorkerMsg::Launched(Err(format!(
                    "webview2 loader failed: {e}"
                ))));
                return;
            }
            let launched = match &join_url {
                Some(u) => roblox::launch_with_url(&latest, u),
                None => roblox::launch(&latest, &extra_args),
            };
            match launched {
                Ok(()) => {
                    let _ = tx.send(WorkerMsg::Launched(Ok(
                        "roblox launched. have fun!".to_string(),
                    )));
                }
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Launched(Err(format!(
                        "launch failed: {e}"
                    ))));
                }
            }
        });
    }

    pub fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                WorkerMsg::Log(line) => self.push_log(line),
                WorkerMsg::Versions { installed, latest } => {
                    self.installed = installed;
                    self.latest = latest;
                    self.busy = false;
                    self.refresh_flags();
                    self.push_log("version info refreshed.".to_string());
                }
                WorkerMsg::Progress { done, total } => {
                    let Some(pct) = done.checked_mul(100).and_then(|d| d.checked_div(total))
                    else {
                        return;
                    };
                    if self.updating {
                        self.update_pct = pct as u8;
                    }
                    // the update screen has its own bar, no need to spam the log
                    if !self.updating && pct < 100 && pct / 25 > self.logged_pct / 25 {
                        self.logged_pct = pct;
                        self.push_log(format!("downloaded {pct}%..."));
                    }
                }
                WorkerMsg::InstallDone(Ok(guid)) => {
                    self.installed = Some(guid.clone());
                    self.busy = false;
                    self.refresh_flags();
                    self.push_log(format!("ready: {guid}"));
                }
                WorkerMsg::InstallDone(Err(e)) => {
                    self.busy = false;
                    self.push_log(format!("install failed: {e}"));
                }
                WorkerMsg::Launched(Ok(line)) => {
                    self.busy = false;
                    self.push_log(line);
                    self.close_at = Some(
                        std::time::Instant::now() + std::time::Duration::from_secs(3),
                    );
                }
                WorkerMsg::Launched(Err(e)) => {
                    self.busy = false;
                    self.push_log(format!("launch failed: {e}"));
                }
                WorkerMsg::Uninstalled(lines) => {
                    self.busy = false;
                    self.installed = None;
                    for line in lines {
                        self.push_log(line);
                    }
                    self.refresh_flags();
                }
                WorkerMsg::WebviewFixed(lines) => {
                    self.busy = false;
                    for line in lines {
                        self.push_log(line);
                    }
                    self.push_log(Self::webview2_summary());
                    self.refresh_flags();
                }
                WorkerMsg::UpdateCheck(Ok(Some(rel))) => {
                    self.checked_at = unix_now();
                    self.checked_for = crate::update::current().to_string();
                    self.seen = Some(rel.clone());
                    self.save();
                    self.offer_update(rel);
                }
                WorkerMsg::UpdateCheck(Ok(None)) => {
                    self.checked_at = unix_now();
                    self.checked_for = crate::update::current().to_string();
                    self.seen = None;
                    self.save();
                    self.push_log("cheesestrap is up to date.".to_string());
                }
                WorkerMsg::UpdateCheck(Err(e)) => {
                    // Offline, throttled or github having a bad day. Count it
                    // as a check anyway, otherwise every launch hammers a
                    // server that already said no, and once the hour is up
                    // nobody will be asking at all.
                    self.checked_at = unix_now();
                    self.checked_for = crate::update::current().to_string();
                    self.save();
                    self.push_log(format!("update check skipped: {e}"));
                }
                WorkerMsg::UpdateDownloaded(Ok(path)) => {
                    match crate::update::spawn_setup(&path) {
                        Ok(()) => {
                            self.update_pct = 100;
                            self.push_log(
                                "setup ready, installing. this window will close.".to_string(),
                            );
                            // linger a beat so the updating state is readable
                            self.close_at = Some(
                                std::time::Instant::now()
                                    + std::time::Duration::from_millis(1400),
                            );
                        }
                        Err(e) => {
                            self.updating = false;
                            self.push_log(e);
                        }
                    }
                }
                WorkerMsg::UpdateDownloaded(Err(e)) => {
                    self.updating = false;
                    self.push_log(format!("update failed: {e}"));
                }
            }
        }
    }
}

fn utc_now_parts() -> (i32, u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86400) as i64;
    let tod = (secs % 86400) as u32;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y } as i32;
    (y, m, d, tod / 3600, (tod % 3600) / 60, tod % 60)
}

fn append_log_file(line: &str) {
    let Some(mut dir) = crate::roblox::app_base_dir().map(|d| d.join("logs")) else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let (y, m, d, hh, mm, ss) = utc_now_parts();
    dir.push(format!("log-{y}-{m:02}-{d:02}.txt"));
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir)
    {
        let _ = writeln!(f, "[{hh:02}:{mm:02}:{ss:02} UTC] {line}");
    }
}
