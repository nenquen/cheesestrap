use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::Sender;

use crate::app::WorkerMsg;

const VERSION_API: &str =
    "https://clientsettingscdn.roblox.com/v2/client-version/WindowsPlayer";
const SETUP_CDN: &str = "https://setup.rbxcdn.com";
const USER_AGENT: &str = "Cheesestrap/1.0.0";

const APP_SETTINGS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<Settings>\r\n\t<ContentFolder>content</ContentFolder>\r\n\t<BaseUrl>http://www.roblox.com</BaseUrl>\r\n</Settings>\r\n";

fn http() -> &'static reqwest::blocking::Client {
    static C: std::sync::OnceLock<reqwest::blocking::Client> =
        std::sync::OnceLock::new();
    C.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("http client")
    })
}

pub fn app_base_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

pub fn clients_dir() -> Option<PathBuf> {
    app_base_dir().map(|d| d.join("clients"))
}

pub fn installed_guids() -> Vec<String> {
    let mut out = Vec::new();
    let Some(dir) = clients_dir() else {
        return out;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        if let Some(name) = e.file_name().to_str() {
            if name.starts_with("version-") && e.path().join("RobloxPlayerBeta.exe").exists() {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    out
}

pub fn latest_installed() -> Option<String> {
    installed_guids().pop()
}

pub fn client_ok(guid: &str) -> bool {
    let Some(dir) = clients_dir().map(|d| d.join(guid)) else {
        return false;
    };
    dir.join("RobloxPlayerBeta.exe").exists() && dir.join("content").exists()
}

pub fn player_exe(guid: &str) -> Option<PathBuf> {
    clients_dir().map(|d| d.join(guid).join("RobloxPlayerBeta.exe"))
}

pub fn fetch_latest_guid() -> Result<String, String> {
    let res: serde_json::Value = http()
        .get(VERSION_API)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    res.get("clientVersionUpload")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "bad version response".to_string())
}

struct Package {
    name: String,
    packed: u64,
}

fn fetch_manifest(guid: &str) -> Result<Vec<Package>, String> {
    let url = format!("{SETUP_CDN}/{guid}-rbxPkgManifest.txt");
    let text = http()
        .get(&url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())?;
    let mut lines = text.lines();
    match lines.next() {
        Some("v0") => {}
        _ => return Err("bad package manifest".to_string()),
    }
    let rest: Vec<&str> = lines.collect();
    let mut out = Vec::new();
    for chunk in rest.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let name = chunk[0].trim().to_string();
        let packed = chunk[2].trim().parse::<u64>().unwrap_or(0);
        out.push(Package { name, packed });
    }
    if out.is_empty() {
        return Err("empty package manifest".to_string());
    }
    Ok(out)
}

fn package_subdir(name: &str) -> Option<&'static str> {
    Some(match name {
        "RobloxApp.zip" | "Libraries.zip" | "redist.zip" | "WebView2.zip" => "",
        "shaders.zip" => "shaders",
        "ssl.zip" => "ssl",
        "content-avatar.zip" => "content/avatar",
        "content-configs.zip" => "content/configs",
        "content-fonts.zip" => "content/fonts",
        "content-sky.zip" => "content/sky",
        "content-sounds.zip" => "content/sounds",
        "content-textures2.zip" => "content/textures",
        "content-models.zip" => "content/models",
        "content-textures3.zip" => "PlatformContent/pc/textures",
        "content-terrain.zip" => "PlatformContent/pc/terrain",
        "content-platform-fonts.zip" => "PlatformContent/pc/fonts",
        "content-platform-dictionaries.zip" => {
            "PlatformContent/pc/shared_compression_dictionaries"
        }
        "extracontent-luapackages.zip" => "ExtraContent/LuaPackages",
        "extracontent-translations.zip" => "ExtraContent/translations",
        "extracontent-models.zip" => "ExtraContent/models",
        "extracontent-textures.zip" => "ExtraContent/textures",
        "extracontent-places.zip" => "ExtraContent/places",
        _ => return None,
    })
}

fn download_to(
    url: &str,
    dest: &PathBuf,
    tx: &Sender<WorkerMsg>,
    base: u64,
    total: u64,
) -> Result<(), String> {
    let mut res = http().get(url).send().map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("download failed: {}", res.status()));
    }
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 65536];
    let mut done: u64 = 0;
    loop {
        let n = res.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        let _ = tx.send(WorkerMsg::Progress {
            done: base + done,
            total,
        });
    }
    Ok(())
}

pub fn download_client(guid: &str, tx: &Sender<WorkerMsg>) -> Result<(), String> {
    let pkgs: Vec<Package> = fetch_manifest(guid)?
        .into_iter()
        .filter(|p| !p.name.ends_with(".exe") && !p.name.contains("RuntimeInstaller"))
        .collect();
    if pkgs.is_empty() {
        return Err("empty package manifest".to_string());
    }
    let total: u64 = pkgs.iter().map(|p| p.packed).sum();
    let dir = clients_dir().ok_or("no local app data")?.join(guid);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let res = download_all(guid, &pkgs, total, &dir, tx);
    if res.is_err() {
        let _ = fs::remove_dir_all(&dir);
    }
    res
}

fn download_all(
    guid: &str,
    pkgs: &[Package],
    total: u64,
    dir: &PathBuf,
    tx: &Sender<WorkerMsg>,
) -> Result<(), String> {
    let mut base: u64 = 0;
    for (i, pkg) in pkgs.iter().enumerate() {
        let _ = tx.send(WorkerMsg::Log(format!(
            "downloading {} ({}/{})...",
            pkg.name,
            i + 1,
            pkgs.len()
        )));
        let url = format!("{SETUP_CDN}/{guid}-{}", pkg.name);
        let tmp = std::env::temp_dir().join(format!("cheesestrap-{}.zip", pkg.name));
        download_to(&url, &tmp, tx, base, total)?;
        let file = fs::File::open(&tmp).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        match package_subdir(&pkg.name) {
            Some("") | None => {
                archive.extract(&dir).map_err(|e| e.to_string())?;
            }
            Some(sub) => {
                let target = dir.join(sub);
                fs::create_dir_all(&target).map_err(|e| e.to_string())?;
                archive.extract(&target).map_err(|e| e.to_string())?;
            }
        }
        let _ = fs::remove_file(&tmp);
        base += pkg.packed;
    }
    fs::write(dir.join("AppSettings.xml"), APP_SETTINGS).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn uninstall() -> Result<Vec<String>, String> {
    let mut done: Vec<String> = Vec::new();
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "RobloxPlayerBeta.exe"])
        .output();
    if let Some(dir) = clients_dir() {
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
            done.push("removed: our clients folder.".to_string());
        }
    }
    if let Some(dir) = dirs::data_local_dir().map(|d| d.join("Roblox")) {
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
            done.push("removed: official roblox appdata folder.".to_string());
        }
    }
    done.push(format!("removed: {} registry entries.", delete_roblox_registry()));
    let (shortcuts, temps) = (delete_roblox_shortcuts(), delete_roblox_temp());
    if shortcuts > 0 {
        done.push(format!("removed: {shortcuts} shortcut(s)."));
    }
    if temps > 0 {
        done.push(format!("removed: {temps} temp file(s)."));
    }
    if done.is_empty() {
        done.push("nothing to uninstall.".to_string());
    }
    Ok(done)
}

fn delete_roblox_registry() -> usize {
    use winreg::enums::*;
    let mut n = 0usize;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    if hkcu
        .delete_subkey_all("SOFTWARE\\ROBLOX Corporation")
        .is_ok()
    {
        n += 1;
    }
    if let Ok(classes) = hkcu.open_subkey_with_flags("SOFTWARE\\Classes", KEY_READ) {
        let names: Vec<String> = classes
            .enum_keys()
            .filter_map(|r| r.ok())
            .filter(|k| k.to_ascii_lowercase().starts_with("roblox"))
            .collect();
        for k in names {
            if hkcu
                .delete_subkey_all(format!("SOFTWARE\\Classes\\{k}"))
                .is_ok()
            {
                n += 1;
            }
        }
    }
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let hk = winreg::RegKey::predef(hive);
        let path = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
        if let Ok(un) = hk.open_subkey_with_flags(path, KEY_READ) {
            let subs: Vec<String> =
                un.enum_keys().filter_map(|r| r.ok()).collect();
            for s in subs {
                let disp = un
                    .open_subkey(&s)
                    .ok()
                    .and_then(|k| k.get_value::<String, _>("DisplayName").ok())
                    .unwrap_or_default();
                if disp.to_ascii_lowercase().contains("roblox")
                    && hk.delete_subkey_all(format!("{path}\\{s}")).is_ok()
                {
                    n += 1;
                }
            }
        }
    }
    n
}

fn delete_roblox_shortcuts() -> usize {
    let mut dirs = Vec::new();
    if let Some(d) = dirs::desktop_dir() {
        dirs.push(d);
    }
    if let Some(d) =
        dirs::data_dir().map(|p| p.join("Microsoft").join("Windows").join("Start Menu").join("Programs"))
    {
        dirs.push(d);
    }
    let mut n = 0usize;
    let mut stack = dirs;
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if let Some(name) = e.file_name().to_str() {
                    if name.to_ascii_lowercase().contains("roblox")
                        && name.to_ascii_lowercase().ends_with(".lnk")
                        && fs::remove_file(&p).is_ok()
                    {
                        n += 1;
                    }
                }
            }
        }
    }
    n
}

fn delete_roblox_temp() -> usize {
    let tmp = std::env::temp_dir();
    let Ok(entries) = fs::read_dir(&tmp) else {
        return 0;
    };
    let mut n = 0usize;
    for e in entries.flatten() {
        if let Some(name) = e.file_name().to_str() {
            let lower = name.to_ascii_lowercase();
            if lower.starts_with("roblox") || lower.starts_with("cheesestrap-") {
                let p = e.path();
                let ok = if p.is_dir() {
                    fs::remove_dir_all(&p).is_ok()
                } else {
                    fs::remove_file(&p).is_ok()
                };
                if ok {
                    n += 1;
                }
            }
        }
    }
    n
}

pub fn edgewebview_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramW6432"] {
        if let Ok(pf) = std::env::var(var) {
            let dir = PathBuf::from(pf).join("Microsoft").join("EdgeWebView");
            if !out.contains(&dir) {
                out.push(dir);
            }
        }
    }
    out
}

/// Registry key edge update uses to advertise the webview2 runtime.
const WV2_CLIENT_KEY: &str =
    "SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

/// What state the webview2 runtime is in on this machine.
pub enum Webview2State {
    /// The runtime is there and the files behind it exist.
    Ok(String),
    /// Nothing registered, the player has never had webview2 on this box.
    Missing,
    /// Registered but the folder it points at is gone. Edge updates have done
    /// this: the files move to EdgeCore while the stale key stays behind, and
    /// roblox trusts the key, finds no runtime, and shows the install dialog.
    Broken {
        version: String,
        location: String,
    },
}

/// Reads the runtime state. `HKLM` is checked through the 32 bit view too since
/// that is where a per machine install of the x64 runtime registers itself.
pub fn webview2_state() -> Webview2State {
    use winreg::enums::*;
    use winreg::RegKey;

    let mut found: Option<(String, String)> = None;
    let views = [
        (HKEY_LOCAL_MACHINE, false),
        (HKEY_LOCAL_MACHINE, true),
        (HKEY_CURRENT_USER, false),
    ];
    for (hive, wow64) in views {
        let hk = RegKey::predef(hive);
        let path = if wow64 {
            format!("SOFTWARE\\WOW6432Node\\{WV2_CLIENT_KEY}")
        } else {
            WV2_CLIENT_KEY.to_string()
        };
        let Ok(key) = hk.open_subkey(&path) else {
            continue;
        };
        let version = key.get_value::<String, _>("pv").unwrap_or_default();
        let location = key.get_value::<String, _>("location").unwrap_or_default();
        if version.is_empty() && location.is_empty() {
            continue;
        }
        // a key whose location actually holds the runtime wins outright
        if !location.is_empty()
            && PathBuf::from(&location)
                .join("msedgewebview2.exe")
                .exists()
        {
            return Webview2State::Ok(version);
        }
        found = Some((version, location));
    }

    match found {
        Some((version, location)) => {
            // no key pointed at real files, but edgecore may still have them
            if let Some(p) = find_edgecore_runtime() {
                return Webview2State::Ok(p);
            }
            Webview2State::Broken {
                version,
                location,
            }
        }
        None => Webview2State::Missing,
    }
}

/// Newer edge builds stash the shared binaries under EdgeCore/<version>, which
/// is where the payload ended up on a box whose EdgeWebView folder got removed.
fn find_edgecore_runtime() -> Option<String> {
    for base in edgewebview_dirs() {
        let core = base
            .parent()
            .map(|p| p.join("EdgeCore"))
            .unwrap_or_else(|| base.clone());
        let Ok(entries) = fs::read_dir(core) else {
            continue;
        };
        for e in entries.flatten() {
            let exe = e.path().join("msedgewebview2.exe");
            if exe.exists() {
                return e.file_name().to_str().map(|s| s.to_string());
            }
        }
    }
    None
}

const WV2_BOOTSTRAP: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

/// Downloads the evergreen webview2 runtime and installs it silently.
///
/// Returns the log lines it produced. The installer is signed by Microsoft and
/// this needs admin, which the app already elevates for.
pub fn install_webview2_runtime(tx: &Sender<WorkerMsg>) -> Result<Vec<String>, String> {
    tx.send(WorkerMsg::Log(
        "downloading the webview2 runtime installer...".to_string(),
    ))
    .ok();

    let tmp = std::env::temp_dir().join("cheesestrap-webview2-runtime.exe");
    let bytes = http()
        .get(WV2_BOOTSTRAP)
        .send()
        .map_err(|e| format!("could not reach microsoft: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download failed: {e}"))?
        .bytes()
        .map_err(|e| format!("download failed: {e}"))?;
    fs::write(&tmp, &bytes).map_err(|e| format!("could not write the installer: {e}"))?;

    tx.send(WorkerMsg::Log(format!(
        "running the installer ({} MB)...",
        bytes.len() / 1_048_576
    )))
    .ok();

    // --silent hides the UI, --system-level puts the runtime in for everyone
    let out = Command::new(&tmp)
        .args(["/silent", "/install"])
        .output()
        .map_err(|e| format!("could not start the installer: {e}"))?;
    let _ = fs::remove_file(&tmp);

    let log = String::from_utf8_lossy(&out.stdout).to_string();
    let mut lines: Vec<String> = log
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if !out.status.success() {
        lines.push(format!("installer exited with {}", out.status));
    }
    match webview2_state() {
        Webview2State::Ok(v) => {
            let msg = if v.is_empty() {
                "webview2 runtime installed.".to_string()
            } else {
                format!("webview2 runtime installed, version {v}.")
            };
            tx.send(WorkerMsg::Log(msg.clone())).ok();
            lines.push(msg);
            Ok(lines)
        }
        _ => {
            // the bootstrapper may have handed off to an msiexec that outlived
            // us, so say so instead of claiming a hard failure
            lines.push("installer finished, runtime still not detected yet.".to_string());
            Ok(lines)
        }
    }
}

pub fn has_clients() -> bool {
    !installed_guids().is_empty()
}



pub fn loader_dll_path(guid: &str) -> Option<PathBuf> {
    clients_dir().map(|d| d.join(guid).join("WebView2Loader.dll"))
}

/// Makes sure WebView2Loader.dll sits next to the player exe.
/// Repairs older installs that missed it.
pub fn ensure_loader_dll(guid: &str, tx: &Sender<WorkerMsg>) -> Result<(), String> {
    if loader_dll_path(guid).is_some_and(|p| p.exists()) {
        return Ok(());
    }
    let pkgs = fetch_manifest(guid)?;
    let Some(pkg) = pkgs.into_iter().find(|p| p.name == "WebView2.zip") else {
        return Ok(());
    };
    let _ = tx.send(WorkerMsg::Log("fetching webview2 loader...".to_string()));
    let dir = clients_dir().ok_or("no local app data")?.join(guid);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let url = format!("{SETUP_CDN}/{guid}-{}", pkg.name);
    let tmp = std::env::temp_dir().join("cheesestrap-webview2dll.zip");
    download_to(&url, &tmp, tx, 0, pkg.packed)?;
    let file = fs::File::open(&tmp).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    archive.extract(&dir).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&tmp);
    Ok(())
}

pub fn launch(guid: &str, extra_args: &str) -> Result<(), String> {
    launch_with(guid, &["--app"], extra_args)
}

pub fn launch_with_url(guid: &str, url: &str) -> Result<(), String> {
    launch_with(guid, &[url], "")
}

fn launch_with(guid: &str, base_args: &[&str], extra_args: &str) -> Result<(), String> {
    let exe = player_exe(guid).ok_or("player exe not found")?;
    let mut cmd = Command::new(exe);
    for a in base_args {
        cmd.arg(a);
    }
    for a in extra_args.split_whitespace() {
        cmd.arg(a);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Registers roblox:// and roblox-player:// protocols to this exe,
/// so the website play button launches through us.
pub fn register_protocols() {
    use winreg::enums::*;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let cmd = format!("\"{}\" -player \"%1\"", exe.to_string_lossy());
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    for proto in ["roblox", "roblox-player"] {
        let base = format!("SOFTWARE\\Classes\\{proto}");
        let Ok((key, _)) = hkcu.create_subkey(&base) else {
            continue;
        };
        let _ = key.set_value("", &format!("URL:{proto} Protocol"));
        let _ = key.set_value("URL Protocol", &"");
        if let Ok((cmdkey, _)) = hkcu.create_subkey(format!("{base}\\shell\\open\\command"))
        {
            let cur: String = cmdkey.get_value("").unwrap_or_default();
            if cur != cmd {
                let _ = cmdkey.set_value("", &cmd);
            }
        }
    }
}

/// Extracts a roblox-player:// style launch url from our args, if any.
pub fn protocol_url_from_args() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    for (i, a) in args.iter().enumerate() {
        if a == "-player" {
            if let Some(u) = args.get(i + 1) {
                return Some(u.clone());
            }
        }
        if a.starts_with("roblox-player:") || (a.starts_with("roblox:") && a.len() > 7) {
            return Some(a.clone());
        }
    }
    None
}

pub fn open_folder() -> Result<(), String> {
    let dir = match latest_installed() {
        Some(g) => clients_dir()
            .map(|d| d.join(g))
            .filter(|p| p.exists())
            .or(clients_dir()),
        None => clients_dir(),
    };
    let dir = dir.ok_or("no roblox folder yet")?;
    Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
