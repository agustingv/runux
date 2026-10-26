//! The EA app, installed and run in its own prefix (`Config::ea`) so it can't
//! disturb Battle.net's, and the games it installs.

use crate::config::{self, Config};
use crate::{disc, download, external, wine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::AppHandle;

const INSTALLER_URL: &str =
    "https://origin-a.akamaihd.net/EA-Desktop-Client-Download/installer-releases/EAappInstaller.exe";
/// Holds one folder per client version (`13.813.1.6327\EA Desktop\…`). The
/// `EA Desktop` junction the registry points at can't be followed from Linux.
const CLIENT_ROOT: &str = r"C:\Program Files\Electronic Arts\EA Desktop";
/// Where the EA app installs games unless told otherwise.
const DEFAULT_LIBRARY: &str = r"C:\Program Files\EA Games";

/// Task of the client while it runs, and of its installer.
pub const TASK: &str = "ea";
pub const INSTALL_TASK: &str = "ea-installer";

/// `cfg` must be the EA one (`Config::ea`) in every function taking it.
fn client_dir(cfg: &Config) -> Option<PathBuf> {
    let version = |name: &str| -> Option<Vec<u32>> { name.split('.').map(|p| p.parse().ok()).collect() };
    std::fs::read_dir(wine::to_unix_path(cfg, CLIENT_ROOT))
        .ok()?
        .flatten()
        .filter_map(|e| Some((version(e.file_name().to_str()?)?, e.path().join("EA Desktop"))))
        .filter(|(_, dir)| dir.join("EADesktop.exe").is_file())
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, dir)| dir)
}

pub fn installed(cfg: &Config) -> bool {
    client_dir(cfg).is_some()
}

pub fn running(cmdlines: &[String]) -> bool {
    cmdlines.iter().any(|c| c.contains("eadesktop.exe"))
}

/// The client and its services (`EABackgroundService.exe`, …), which all
/// live in its install folder, plus the runner wrappers launching them.
fn is_ea_process(cmdline: &str) -> bool {
    cmdline.contains("/electronic arts/ea desktop/")
}

/// Creates the prefix if needed, then downloads and runs the official
/// installer; its `task-finished` event tells when it's done.
pub async fn install(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    if !wine::prefix_exists(cfg) {
        wine::create_prefix(app, cfg, INSTALL_TASK).await?;
    }
    wine::log(app, INSTALL_TASK, "Descargando el instalador de la EA app…");
    let setup = download::fetch(app, INSTALL_TASK, INSTALLER_URL).await?;
    let cache = config::cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let setup_path = cache.join("EAappInstaller.exe");
    std::fs::write(&setup_path, setup).map_err(|e| e.to_string())?;

    register_dotnet(app, cfg).await?;
    wine::log(app, INSTALL_TASK, "Ejecutando el instalador. Sigue los pasos en su ventana.");
    let mut cmd = wine::command(cfg)?;
    cmd.arg(&setup_path);
    wine::spawn_logged(app, INSTALL_TASK, cmd);
    Ok(())
}

/// Where .NET is installed, which the installer's .NET custom actions look
/// up. GE-Proton's prefixes run Wine Mono from the Proton build without these
/// keys, and the MSI then fails with error 1603 (`0x80070643`).
const DOTNET_INSTALL_ROOTS: &[(&str, &str)] = &[
    ("/reg:64", r"C:\windows\Microsoft.NET\Framework64\"),
    ("/reg:32", r"C:\windows\Microsoft.NET\Framework\"),
];

async fn register_dotnet(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    for (view, root) in DOTNET_INSTALL_ROOTS {
        let mut cmd = wine::tool_command(cfg)?;
        cmd.args(["reg", "add", r"HKLM\Software\Microsoft\.NETFramework", "/v", "InstallRoot", "/d", root, "/f", view])
            .kill_on_drop(true);
        tokio::time::timeout(std::time::Duration::from_secs(60), wine::run_logged(app, INSTALL_TASK, cmd))
            .await
            .map_err(|_| "Wine no respondió al preparar el prefijo de la EA app".to_string())??;
    }
    Ok(())
}

pub fn launch(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let dir = client_dir(cfg).ok_or("La EA app no está instalada")?;
    if running(&wine::running_cmdlines()) {
        return Err("La EA app ya está abierta".into());
    }
    let mut cmd = wine::command(cfg)?;
    cmd.arg(dir.join("EADesktop.exe"));
    wine::spawn_logged(app, TASK, cmd);
    *CLIENT_ENV.lock().unwrap() = Some(BTreeMap::new());
    Ok(())
}

pub async fn close() -> usize {
    *CLIENT_ENV.lock().unwrap() = None;
    wine::close_processes(is_ea_process).await
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EaGame {
    /// Slug of its folder name, unique in the library.
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    /// Content IDs from its manifest, which the client launches it by.
    pub content_ids: Vec<String>,
    /// Executable run directly when the manifest has no content IDs.
    pub exe: Option<PathBuf>,
    pub accent: String,
    /// Extra environment variables, passed on through the client.
    pub env: BTreeMap<String, String>,
    pub running: bool,
}

/// A Windows path inside the prefix, resolved case-insensitively.
fn resolve(cfg: &Config, win: &str) -> Option<PathBuf> {
    let (drive, rest) = win.split_once(':')?;
    disc::find_ci(&wine::to_unix_path(cfg, &format!("{drive}:")), rest)
}

fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xFF, 0xFE, rest @ ..] => {
            let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&units)
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'")
}

/// The text of the first `<tag …>` element after `from` in `xml`.
fn element<'a>(xml: &'a str, from: usize, tag: &str) -> Option<&'a str> {
    let open = from + xml[from..].find(&format!("<{tag}"))?;
    let start = open + xml[open..].find('>')? + 1;
    let end = start + xml[start..].find(&format!("</{tag}>"))?;
    Some(&xml[start..end])
}

/// The game's title from `installerdata.xml`, in Spanish when available.
/// Older manifests use `<localeInfo locale="…"><title>`, newer ones
/// `<gameTitle locale="…">`.
fn manifest_title(xml: &str) -> Option<String> {
    let in_locale = |locale: &str| {
        xml.find(&format!("<localeInfo locale=\"{locale}\""))
            .and_then(|i| element(xml, i, "title"))
            .or_else(|| {
                let i = xml.find(&format!("<gameTitle locale=\"{locale}\""))?;
                element(xml, i, "gameTitle")
            })
    };
    ["es_ES", "en_US"]
        .iter()
        .find_map(|l| in_locale(l))
        .or_else(|| element(xml, 0, "title"))
        .or_else(|| element(xml, 0, "gameTitle"))
        .map(|t| unescape(t.trim()))
        .filter(|t| !t.is_empty())
}

/// The `<contentIDs>` of the manifest.
fn manifest_content_ids(xml: &str) -> Vec<String> {
    let Some(list) = element(xml, 0, "contentIDs") else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    let mut from = 0;
    while let Some(id) = list.get(from..).and_then(|_| element(list, from, "contentID")) {
        let id = id.trim();
        if !id.is_empty() {
            ids.push(id.to_string());
        }
        from = list[from..].find("</contentID>").map_or(list.len(), |i| from + i + "</contentID>".len());
    }
    ids
}

/// The game executable in the manifest's `<runtime>` launchers, relative to
/// the install folder (`[HKEY_…\Install Dir]Game.exe` → `Game.exe`).
fn manifest_exe(xml: &str) -> Option<String> {
    let runtime = xml.find("<runtime>")?;
    let path = unescape(element(xml, runtime, "filePath")?.trim());
    let rel = match path.strip_prefix('[') {
        Some(rest) => rest.split_once(']')?.1.to_string(),
        None => path,
    };
    Some(rel.trim_start_matches(['\\', '/']).to_string())
}

/// Every `.exe` in the game's folder, outside its own installer.
fn guess_exe(dir: &Path) -> Option<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = e.path();
            let Ok(kind) = e.file_type() else {
                continue;
            };
            if kind.is_dir() && depth > 0 && !e.file_name().eq_ignore_ascii_case("__Installer") {
                walk(&path, depth - 1, out);
            } else if kind.is_file() && path.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
                out.push(path);
            }
        }
    }
    let mut exes = Vec::new();
    walk(dir, 4, &mut exes);
    exes.into_iter().min_by_key(|p| disc::game_rank(p))
}

fn game_at(cfg: &Config, dir: &Path, cmdlines: &[String]) -> Option<EaGame> {
    let xml = decode(&std::fs::read(disc::find_ci(dir, r"__Installer\installerdata.xml")?).ok()?);
    let folder = dir.file_name()?.to_string_lossy().into_owned();
    let exe = manifest_exe(&xml)
        .and_then(|rel| disc::find_ci(dir, &rel))
        .or_else(|| guess_exe(dir));
    let inside = format!("{}/", wine::to_win_path(cfg, dir));
    Some(EaGame {
        id: external::slug(&folder),
        name: manifest_title(&xml).unwrap_or(folder),
        path: dir.to_path_buf(),
        content_ids: manifest_content_ids(&xml),
        exe,
        accent: String::new(),
        env: BTreeMap::new(),
        running: cmdlines.iter().any(|c| c.contains(&inside)),
    })
}

/// The client's per-account settings (`user_<id>.ini`), in each prefix
/// user's `AppData` (`steamuser` with Proton, the Linux user with Wine).
/// They only exist once someone has signed in.
fn user_settings(cfg: &Config) -> Vec<PathBuf> {
    let users = wine::to_unix_path(cfg, r"C:\users");
    std::fs::read_dir(users)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|user| std::fs::read_dir(user.path().join("AppData/Local/Electronic Arts/EA Desktop")).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            name.starts_with("user_") && name.ends_with(".ini")
        })
        .collect()
}

fn ini_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

/// `text` with `key` set to `value`, keeping its line endings.
fn set_ini_value(text: &str, key: &str, value: &str) -> String {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut found = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| match l.split_once('=') {
            Some((k, _)) if k.trim() == key => {
                found = true;
                format!("{key}={value}")
            }
            _ => l.to_string(),
        })
        .collect();
    if !found {
        lines.push(format!("{key}={value}"));
    }
    lines.join(eol) + eol
}

/// The download folder chosen in the client's settings, if it was changed.
fn custom_library(cfg: &Config) -> Option<String> {
    user_settings(cfg).iter().find_map(|p| {
        ini_value(&std::fs::read_to_string(p).ok()?, "user.downloadinplacedir").filter(|v| !v.is_empty())
    })
}

const OVERLAY_KEY: &str = "user.igoenabled";

/// Whether the client's in-game overlay is on (its default), or `None`
/// before anyone has signed in.
pub fn overlay_enabled(cfg: &Config) -> Option<bool> {
    let files = user_settings(cfg);
    if files.is_empty() {
        return None;
    }
    Some(files.iter().any(|p| {
        std::fs::read_to_string(p).map_or(true, |t| ini_value(&t, OVERLAY_KEY).as_deref() != Some("0"))
    }))
}

/// Turns the in-game overlay on or off. It hooks into the games to draw
/// over them and, under Wine, can swallow their keyboard and mouse input.
pub fn set_overlay(cfg: &Config, enabled: bool) -> Result<(), String> {
    if running(&wine::running_cmdlines()) {
        return Err("Cierra la EA app antes: guarda sus ajustes al salir y desharía el cambio".into());
    }
    let files = user_settings(cfg);
    if files.is_empty() {
        return Err("Abre la EA app e inicia sesión primero".into());
    }
    for p in files {
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        std::fs::write(&p, set_ini_value(&text, OVERLAY_KEY, if enabled { "1" } else { "0" }))
            .map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(())
}

/// Games installed by the EA app: folders with an `__Installer` manifest in
/// its default library or the download folder set in its settings.
pub fn games(cfg: &Config, cmdlines: &[String]) -> Vec<EaGame> {
    if !wine::prefix_exists(cfg) {
        return Vec::new();
    }
    let mut libraries: Vec<PathBuf> = std::iter::once(DEFAULT_LIBRARY.to_string())
        .chain(custom_library(cfg))
        .filter_map(|win| resolve(cfg, &win))
        .collect();
    libraries.sort();
    libraries.dedup();
    let mut games: Vec<EaGame> = libraries
        .iter()
        .flat_map(|lib| std::fs::read_dir(lib).into_iter().flatten().flatten())
        .filter_map(|e| game_at(cfg, &e.path(), cmdlines))
        .collect();
    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    games.dedup_by(|a, b| a.id == b.id);
    let mut settings = load_settings();
    for (i, g) in games.iter_mut().enumerate() {
        g.accent = external::ACCENTS[i % external::ACCENTS.len()].into();
        if let Some(s) = settings.remove(&g.id) {
            g.env = s.env;
        }
    }
    games
}

/// What the user set for a game, in `~/.config/runux/ea-games.json` by id.
#[derive(Default, Serialize, Deserialize)]
struct GameSettings {
    #[serde(default)]
    env: BTreeMap<String, String>,
}

fn settings_path() -> PathBuf {
    config::config_dir().join("ea-games.json")
}

fn load_settings() -> BTreeMap<String, GameSettings> {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn set_env(id: &str, env: BTreeMap<String, String>) -> Result<(), String> {
    if let Some(bad) = env.keys().find(|k| !external::valid_env_name(k)) {
        return Err(format!("Nombre de variable no válido: «{bad}»"));
    }
    let mut settings = load_settings();
    if env.is_empty() {
        settings.remove(id);
    } else {
        settings.entry(id.to_string()).or_default().env = env;
    }
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Environment the running client was started with by this app; `None`
/// when it wasn't (or isn't running), so its environment is unknown.
static CLIENT_ENV: Mutex<Option<BTreeMap<String, String>>> = Mutex::new(None);

/// Whether the client must be restarted for `env` to reach a game it
/// launches: games inherit the client's environment, not the request's.
fn needs_restart(client: Option<&BTreeMap<String, String>>, env: &BTreeMap<String, String>) -> bool {
    match client {
        Some(current) => current != env,
        None => !env.is_empty(),
    }
}

pub fn find_game(cfg: &Config, id: &str) -> Result<EaGame, String> {
    games(cfg, &[])
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("Juego de EA desconocido: {id}"))
}

/// Through the client, so it checks the license, syncs saves and applies
/// updates; it takes the manifest's content IDs (as `offerIds`, despite the
/// name) and picks the one the account owns. Without them, runs the executable.
/// The game's environment variables go to the client, which passes them on;
/// it's restarted first if it runs with other ones.
pub async fn launch_game(app: &AppHandle, cfg: &Config, game: &EaGame) -> Result<(), String> {
    let task = format!("ea:{}", game.id);
    let mut cmd = wine::command(cfg)?;
    for (k, v) in &game.env {
        wine::log(app, &task, format!("{k}={v}"));
    }
    match (game.content_ids.is_empty(), &game.exe) {
        (false, _) => {
            let dir = client_dir(cfg).ok_or("La EA app no está instalada")?;
            let client_env = CLIENT_ENV.lock().unwrap().clone();
            if running(&wine::running_cmdlines()) {
                if needs_restart(client_env.as_ref(), &game.env) {
                    wine::log(app, &task, "Reiniciando la EA app para aplicar las variables de entorno del juego");
                    close().await;
                }
            }
            if !running(&wine::running_cmdlines()) {
                *CLIENT_ENV.lock().unwrap() = Some(game.env.clone());
            }
            let ids = game.content_ids.join(",");
            wine::log(app, &task, format!("Lanzando {} a través de la EA app ({ids})", game.name));
            cmd.arg(dir.join("EALauncher.exe"))
                .arg(format!("origin2://game/launch?offerIds={ids}"));
        }
        (true, Some(exe)) => {
            wine::log(app, &task, format!("Lanzando {} ({})", game.name, exe.display()));
            cmd.arg(exe).current_dir(exe.parent().unwrap_or(&game.path));
        }
        (true, None) => return Err(format!("No se encuentra el ejecutable de {}", game.name)),
    }
    // Last, so they can override the runner's own (e.g. `WINEDEBUG`).
    cmd.envs(&game.env);
    wine::spawn_logged(app, &task, cmd);
    Ok(())
}

/// `(program, arguments)` of the uninstaller a game registered under
/// `HKLM\Software[\Wow6432Node]\Electronic Arts\<folder>`, from `reg query`.
fn parse_uninstaller(reg_output: &str) -> Option<(String, String)> {
    let value = |name: &str| {
        reg_output.lines().find_map(|l| {
            let (key, v) = l.split_once("REG_SZ")?;
            (key.trim() == name).then(|| v.trim().to_string())
        })
    };
    let program = value("Uninstall Program")?.trim_matches('"').to_string();
    (!program.is_empty()).then(|| (program, value("Uninstall Arguments").unwrap_or_default()))
}

async fn uninstaller(cfg: &Config, game: &EaGame) -> Result<(String, String), String> {
    let folder = game.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    for root in [r"HKLM\Software\Wow6432Node\Electronic Arts", r"HKLM\Software\Electronic Arts"] {
        let mut cmd = wine::tool_command(cfg)?;
        cmd.args(["reg", "query", &format!("{root}\\{folder}")]).kill_on_drop(true);
        let out = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output())
            .await
            .map_err(|_| "Wine no respondió al buscar el desinstalador".to_string())?
            .map_err(|e| e.to_string())?;
        if let Some(found) = parse_uninstaller(&String::from_utf8_lossy(&out.stdout)) {
            return Ok(found);
        }
    }
    Err(format!("{} no registró ningún desinstalador", game.name))
}

/// Runs the game's own uninstaller. The client can't do it for older
/// (Origin-era) games under Wine: it falls back to opening Windows' "Apps &
/// features" settings, which don't exist there.
pub async fn uninstall_game(app: &AppHandle, cfg: &Config, game: &EaGame) -> Result<(), String> {
    if game.running {
        return Err(format!("Cierra {} antes de desinstalarlo", game.name));
    }
    let (program, args) = uninstaller(cfg, game).await?;
    let task = format!("ea-uninstall:{}", game.id);
    wine::log(app, &task, format!("Desinstalando {} con {program} {args}", game.name));
    // The plain wine binary, so the task ends when the uninstaller does.
    let mut cmd = wine::tool_command(cfg)?;
    cmd.arg(&program).args(args.split_whitespace()).current_dir(cfg.wine_prefix());
    wine::spawn_logged(app, &task, cmd);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_its_processes() {
        assert!(is_ea_process(
            "c:/program files/electronic arts/ea desktop/13.813.1.6327/ea desktop/eabackgroundservice.exe"
        ));
        assert!(is_ea_process(
            "c:/windows/system32/steam.exe /home/u/.local/share/btn/ea/proton/pfx/dosdevices/c:/program files/electronic arts/ea desktop/13.813.1.6327/ea desktop/eadesktop.exe"
        ));
        assert!(!is_ea_process("c:/program files (x86)/battle.net/battle.net.exe"));
    }

    #[test]
    fn finds_the_newest_client() {
        let pfx = std::env::temp_dir().join(format!("runux-ea-{}", std::process::id()));
        let root = pfx.join("dosdevices/c:/Program Files/Electronic Arts/EA Desktop");
        for v in ["13.9.0.1", "13.813.1.6327", "EA Desktop?"] {
            let dir = root.join(v).join("EA Desktop");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("EADesktop.exe"), b"").unwrap();
        }
        let cfg = Config { runner: config::Runner::Wine, prefix: pfx.clone(), ..Default::default() };
        let found = client_dir(&cfg);
        std::fs::remove_dir_all(&pfx).unwrap();
        assert_eq!(found, Some(root.join("13.813.1.6327/EA Desktop")));
    }

    #[test]
    fn restarts_the_client_only_for_other_variables() {
        let none = BTreeMap::new();
        let wined3d = BTreeMap::from([("PROTON_USE_WINED3D".to_string(), "1".to_string())]);
        // Started elsewhere: only a game with variables needs a restart.
        assert!(!needs_restart(None, &none));
        assert!(needs_restart(None, &wined3d));
        // Started by us with known variables: restart when they differ.
        assert!(!needs_restart(Some(&none), &none));
        assert!(needs_restart(Some(&none), &wined3d));
        assert!(!needs_restart(Some(&wined3d), &wined3d));
        assert!(needs_restart(Some(&wined3d), &none));
    }

    #[test]
    fn edits_client_settings() {
        let ini = "user.userid=1\r\nuser.igoenabled=1\r\n";
        assert_eq!(ini_value(ini, "user.igoenabled").as_deref(), Some("1"));
        assert_eq!(set_ini_value(ini, "user.igoenabled", "0"), "user.userid=1\r\nuser.igoenabled=0\r\n");
        assert_eq!(set_ini_value("user.userid=1\n", "user.igoenabled", "0"), "user.userid=1\nuser.igoenabled=0\n");
    }

    #[test]
    fn finds_the_uninstaller() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\Software\\Wow6432Node\\Electronic Arts\\Mass Effect 3\r\n    Uninstall Arguments    REG_SZ    uninstall_game -autologging\r\n    Uninstall Program    REG_SZ    \"C:\\Program Files (x86)\\Common Files\\EAInstaller\\Mass Effect 3\\Cleanup.exe\"\r\n";
        assert_eq!(
            parse_uninstaller(out),
            Some((
                r"C:\Program Files (x86)\Common Files\EAInstaller\Mass Effect 3\Cleanup.exe".into(),
                "uninstall_game -autologging".into()
            ))
        );
        assert_eq!(parse_uninstaller("ERROR: no se encuentra la clave"), None);
    }

    #[test]
    fn reads_manifests() {
        let old = r#"<game><contentIDs>
    <contentID>71073</contentID>
    <contentID>71183</contentID>
  </contentIDs><metadata><localeInfo locale="en_US"><title>Mass Effect™ 3</title></localeInfo>
            <localeInfo locale="es_ES"><title>Mass Effect™ 3 (ES)</title></localeInfo></metadata></game>"#;
        assert_eq!(manifest_title(old).as_deref(), Some("Mass Effect™ 3 (ES)"));
        assert_eq!(manifest_exe(old), None);
        assert_eq!(manifest_content_ids(old), ["71073", "71183"]);
        let new = r#"<DiPManifest><gameTitles><gameTitle locale="en_US">Dead Space</gameTitle></gameTitles>
            <runtime><launcher><filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\EA Games\Dead Space\Install Dir]Dead Space.exe</filePath></launcher></runtime></DiPManifest>"#;
        assert_eq!(manifest_title(new).as_deref(), Some("Dead Space"));
        assert_eq!(manifest_exe(new).as_deref(), Some("Dead Space.exe"));
        assert!(manifest_content_ids(new).is_empty());
    }
}


