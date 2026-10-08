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
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SavedSettings {
    #[serde(default)]
    logs_to_file: bool,
    #[serde(default = "default_true")]
    show_hints: bool,
}

fn default_true() -> bool {
    true
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
    pub can_clean_webview: bool,
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
    pub tx: Sender<WorkerMsg>,
    pub rx: Receiver<WorkerMsg>,
}

impl App {
    pub fn new(tx: Sender<WorkerMsg>, rx: Receiver<WorkerMsg>) -> Self {
        let saved = load_saved();
        Self {
            focus: Focus::Menu,
            using_keyboard: false,
            logged_pct: 0,
            menu_idx: 0,
            action_idx: 0,
            settings_idx: 0,
            logs_to_file: saved.as_ref().map(|s| s.logs_to_file).unwrap_or(false),
            show_hints: saved.as_ref().map(|s| s.show_hints).unwrap_or(true),
            close_at: None,
            can_uninstall: roblox::has_clients(),
            can_clean_webview: roblox::has_webview_traces(),
            log_view_end: None,
            hover: None,
            protocol_url: None,
            extra_args: String::new(),
            editing_args: false,
            installed: None,
            latest: None,
            log: vec!["welcome to cheesestrap.".to_string()],
            busy: false,
            ctx_close_requested: false,
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
        self.can_clean_webview = roblox::has_webview_traces();
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

    pub fn delete_webview2(&mut self) {
        let removed = roblox::remove_webview_everywhere();
        if removed.is_empty() {
            self.push_log("no webview2 traces found.".to_string());
        } else {
            for r in removed {
                match r {
                    roblox::Removed::File(name) => {
                        self.push_log(format!("removed: {name}"))
                    }
                    roblox::Removed::Dir(name) => {
                        self.push_log(format!("removed: {name}"))
                    }
                    roblox::Removed::Denied(name) => self.push_log(format!(
                        "access denied: {name} (already admin?)"
                    )),
                }
            }
        }
        self.refresh_flags();
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
                    if total > 0 {
                        let pct = done * 100 / total;
                        if pct / 25 > self.logged_pct / 25 && pct < 100 {
                            self.logged_pct = pct;
                            self.push_log(format!("downloaded {pct}%..."));
                        }
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
