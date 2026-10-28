//! Games that don't come from Battle.net: a Windows executable run in the
//! prefix, or a game an emulator runs (see `emulator`). Stored in
//! `~/.config/runux/games.json`.

use crate::config::{self, Config};
use crate::emulator::{self, Media};
use crate::task::Task;
use crate::{store, wine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};
use tokio::process::Command;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename = "ExternalKind")]
/// What runs the game: Wine/Proton for `Windows`, an emulator for the rest.
pub enum Kind {
    Scummvm,
    Windows,
    Dreamcast,
    Playstation,
    Playstation2,
    Playstation3,
    Psp,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ExternalGameEntry")]
pub struct ExternalGame {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    /// Game directory (the ScummVM path, or the folder holding the executable
    /// or the disc image).
    pub path: PathBuf,
    /// Windows executable, for `Kind::Windows`.
    #[serde(default)]
    pub exe: Option<PathBuf>,
    /// Disc image or game folder the emulator opens, for emulators of `Media::File`.
    #[serde(default)]
    pub rom: Option<PathBuf>,
    /// ScummVM target, e.g. `bladerunner:bladerunner`.
    #[serde(default)]
    pub scummvm_id: Option<String>,
    pub accent: String,
    /// Whether `path` was extracted by the launcher (and is removed with the game).
    #[serde(default)]
    pub imported: bool,
    /// Extra environment variables for a Windows game's process, e.g.
    /// `WINEDLLOVERRIDES` or `PROTON_USE_WINED3D`.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The emulator's settings for this game (`emulator::Fix::key` → value);
    /// `None` until changed, for the emulator's defaults. `flycast` held them
    /// when only Flycast had any.
    #[serde(default, alias = "flycast")]
    pub settings: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ExternalGame")]
pub struct ExternalStatus {
    #[serde(flatten)]
    pub game: ExternalGame,
    pub running: bool,
}

#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "AddGameRequest")]
pub struct AddRequest {
    pub kind: Kind,
    /// Empty to use the detected (ScummVM) or file name.
    pub name: String,
    /// ISO images to extract, or a single game directory, for emulators of
    /// `Media::GameFiles`.
    pub sources: Vec<PathBuf>,
    /// Windows executable, for `Kind::Windows`.
    pub exe: Option<PathBuf>,
    /// Disc image or game folder, for emulators of `Media::File`.
    pub rom: Option<PathBuf>,
}

const TASK: &str = Task::Import.name();

pub(crate) const ACCENTS: &[&str] = &["#c2410c", "#7c3aed", "#0f766e", "#b91c1c", "#1d4ed8", "#a16207"];

fn store_path() -> PathBuf {
    config::config_dir().join("games.json")
}

/// Held from loading `games.json` to saving it, so that two changes at once
/// (adding a game while editing another) don't undo each other.
static STORE: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    STORE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The games this version can run; others, added by a newer version, are
/// kept in the file but left out.
pub fn load() -> Vec<ExternalGame> {
    store::load_list(&store_path()).0
}

fn save(games: &[ExternalGame]) -> Result<(), String> {
    store::save_list(&store_path(), games)
}

pub fn find(id: &str) -> Result<ExternalGame, String> {
    load()
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("Juego desconocido: {id}"))
}

/// The games, with the settings their emulators run them with.
pub fn list(cfg: &Config, cmdlines: &[String]) -> Vec<ExternalStatus> {
    load()
        .into_iter()
        .map(|mut game| {
            let running = cmdlines.iter().any(|c| c.contains(&running_marker(cfg, &game)));
            if let Some(emu) = emulator::get(game.kind) {
                game.settings = Some(emu.settings(&game));
            }
            ExternalStatus { game, running }
        })
        .collect()
}

/// What the command line of a running game contains. A Windows game runs
/// from its executable's path as Wine names it, ending in a separator so
/// `C:/Games/Alive` doesn't match `C:/Games/Alive2`; emulators say theirs.
fn running_marker(cfg: &Config, game: &ExternalGame) -> String {
    match emulator::get(game.kind) {
        Some(emu) => emu.running_marker(game),
        None => format!("{}/", wine::to_win_path(cfg, &game.path)),
    }
}

fn has_extension(p: &Path, extensions: &[String]) -> bool {
    p.extension().is_some_and(|e| extensions.iter().any(|ext| e.eq_ignore_ascii_case(ext)))
}

pub(crate) fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "juego".into() } else { s }
}

fn unique_id(name: &str, games: &[ExternalGame]) -> String {
    let base = slug(name);
    let mut id = base.clone();
    let mut n = 2;
    // Games left out by `load` hold their ids too.
    let (_, others) = store::load_list::<ExternalGame>(&store_path());
    let taken = |id: &str| {
        games.iter().any(|g| g.id == id)
            || others.iter().any(|g| g["id"] == id)
            || config::data_dir().join("games").join(id).exists()
    };
    while taken(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

fn emit_progress(app: &AppHandle, done: u64, total: u64) {
    let _ = app.emit(
        "progress",
        serde_json::json!({ "task": TASK, "downloaded": done, "total": total }),
    );
}

/// Extracts every ISO into `dest`, so multi-disc games end up with their
/// `CD1/`, `CD2/`… folders side by side, as ScummVM expects.
async fn extract_isos(app: &AppHandle, isos: &[PathBuf], dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let sizes: Vec<u64> = isos
        .iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .collect();
    let total = sizes.iter().sum();
    let mut done = 0;
    emit_progress(app, 0, total);
    for (i, (iso, size)) in isos.iter().zip(&sizes).enumerate() {
        let name = iso.file_name().unwrap_or_default().to_string_lossy();
        wine::log(app, TASK, format!("Extrayendo {name} ({}/{})", i + 1, isos.len()));
        let mut cmd = Command::new("bsdtar");
        cmd.arg("-xf").arg(iso).arg("-C").arg(dest);
        wine::run_logged(app, TASK, cmd)
            .await
            .map_err(|e| format!("No se pudo extraer {name}: {e}"))?;
        done += size;
        emit_progress(app, done, total);
    }
    // ISO 9660 files come out read-only, which would block removing them later.
    let mut cmd = Command::new("chmod");
    cmd.arg("-R").arg("u+w").arg(dest);
    wine::run_logged(app, TASK, cmd).await
}

fn is_iso(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso"))
}

pub async fn add(app: &AppHandle, req: AddRequest) -> Result<ExternalGame, String> {
    let isos: Vec<PathBuf> = {
        let mut v: Vec<PathBuf> = req.sources.iter().filter(|p| is_iso(p)).cloned().collect();
        v.sort();
        v
    };
    if !isos.is_empty() && isos.len() != req.sources.len() {
        return Err("Elige solo imágenes ISO, o solo una carpeta".into());
    }
    let emu = emulator::get(req.kind);
    // Fail early, before extracting anything, when the program is missing.
    if let Some(emu) = emu {
        emu.require()?;
    }

    let (path, imported) = match emu.map(|e| (e, e.media())) {
        None => {
            let exe = req.exe.as_ref().ok_or("Elige el ejecutable del juego")?;
            let dir = exe.parent().ok_or("Ruta de ejecutable no válida")?;
            (dir.to_path_buf(), false)
        }
        Some((emu, Media::File { extensions, folder, .. })) => {
            let rom = req.rom.as_ref().ok_or("Elige el juego")?;
            if !(has_extension(rom, &extensions) || folder && rom.is_dir()) {
                let what = if folder { "carpetas de juego o imágenes" } else { "imágenes" };
                return Err(format!("{} abre {what} {}", emu.program(), extensions.join(", ")));
            }
            (emu.game_dir(rom), false)
        }
        Some((_, Media::GameFiles)) if isos.is_empty() => match req.sources.as_slice() {
            [dir] if dir.is_dir() => (dir.clone(), false),
            _ => return Err("Elige las imágenes ISO del juego o su carpeta".into()),
        },
        Some((_, Media::GameFiles)) => {
            // Renamed after the game's id once the name is known.
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let dest = config::data_dir().join("games").join(format!(".import-{stamp}"));
            if let Err(e) = extract_isos(app, &isos, &dest).await {
                remove_dir(&dest);
                return Err(e);
            }
            (dest, true)
        }
    };

    let (scummvm_id, detected) = match req.kind {
        Kind::Scummvm => match emulator::detect_scummvm(app, &path).await {
            Ok((id, desc)) => (Some(id), Some(desc)),
            Err(e) => {
                if imported {
                    remove_dir(&path);
                }
                return Err(e);
            }
        },
        _ => (None, emu.zip(req.rom.as_deref()).and_then(|(emu, rom)| emu.game_name(rom))),
    };

    let name = match req.name.trim() {
        "" => detected
            .or_else(|| req.exe.as_ref().and_then(|e| e.file_stem()).map(|s| s.to_string_lossy().into()))
            .unwrap_or_else(|| "Juego".into()),
        n => n.to_string(),
    };
    // Loaded only now: extracting and detecting may have taken minutes, and
    // other games may have changed meanwhile. No `await` from here on.
    let _lock = lock();
    let mut games = load();
    let id = unique_id(&name, &games);
    let path = if imported {
        let dest = config::data_dir().join("games").join(&id);
        std::fs::rename(&path, &dest).map_err(|e| e.to_string())?;
        dest
    } else {
        path
    };
    let game = ExternalGame {
        id,
        accent: ACCENTS[games.len() % ACCENTS.len()].into(),
        name,
        kind: req.kind,
        path,
        exe: req.exe,
        rom: req.rom,
        scummvm_id,
        imported,
        env: BTreeMap::new(),
        settings: None,
    };
    games.push(game.clone());
    save(&games)?;
    wine::log(app, TASK, format!("Añadido: {}", game.name));
    Ok(game)
}

fn remove_dir(dir: &Path) {
    let _ = std::process::Command::new("chmod").arg("-R").arg("u+w").arg(dir).status();
    let _ = std::fs::remove_dir_all(dir);
}

pub fn remove(id: &str) -> Result<(), String> {
    let _lock = lock();
    let mut games = load();
    let index = games
        .iter()
        .position(|g| g.id == id)
        .ok_or_else(|| format!("Juego desconocido: {id}"))?;
    let game = games.remove(index);
    save(&games)?;
    if game.imported {
        remove_dir(&game.path);
    }
    Ok(())
}

fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Rejects names a process environment can't hold, before saving them.
pub(crate) fn check_env(env: &BTreeMap<String, String>) -> Result<(), String> {
    match env.keys().find(|k| !valid_env_name(k)) {
        Some(bad) => Err(format!("Nombre de variable no válido: «{bad}»")),
        None => Ok(()),
    }
}

/// Saves a change to one game, holding the store lock throughout.
fn update(id: &str, change: impl FnOnce(&mut ExternalGame)) -> Result<ExternalGame, String> {
    let _lock = lock();
    let mut games = load();
    let game = games
        .iter_mut()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("Juego desconocido: {id}"))?;
    change(game);
    let game = game.clone();
    save(&games)?;
    Ok(game)
}

pub fn set_env(id: &str, env: BTreeMap<String, String>) -> Result<ExternalGame, String> {
    check_env(&env)?;
    update(id, |game| game.env = env)
}

pub fn set_settings(id: &str, settings: BTreeMap<String, String>) -> Result<ExternalGame, String> {
    let emu = emulator::get(find(id)?.kind).ok_or("Este juego no usa un emulador")?;
    for (key, value) in &settings {
        emu.check_setting(key, value)?;
    }
    update(id, |game| game.settings = Some(settings))
}

pub fn launch(app: &AppHandle, cfg: &Config, game: &ExternalGame) -> Result<(), String> {
    let task = format!("ext:{}", game.id);
    wine::log(app, &task, format!("Lanzando {}", game.name));
    match emulator::get(game.kind) {
        Some(emu) => wine::spawn_logged(app, &task, emu.command(game, &emu.settings(game))?),
        None => {
            let exe = game.exe.as_ref().ok_or("Falta el ejecutable del juego")?;
            let mut cmd = wine::command(cfg)?;
            cmd.arg(exe).current_dir(&game.path);
            wine::spawn_game(app, &task, cmd, &game.env);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_needs_the_whole_folder_name() {
        let cfg = Config { runner: config::Runner::Wine, prefix: "/pfx".into(), ..Default::default() };
        let game = |kind, path: &str| ExternalGame {
            id: "x".into(),
            name: "X".into(),
            kind,
            path: path.into(),
            exe: None,
            rom: None,
            scummvm_id: None,
            accent: String::new(),
            imported: false,
            env: BTreeMap::new(),
            settings: None,
        };
        let alive = running_marker(&cfg, &game(Kind::Windows, "/pfx/drive_c/Games/Alive"));
        assert!("c:/games/alive/alive.exe ".contains(&alive));
        assert!(!"c:/games/alive2/alive2.exe ".contains(&alive));
        let br = running_marker(&cfg, &game(Kind::Scummvm, "/g/BR"));
        assert!("/usr/games/scummvm --path=/g/br bladerunner:bladerunner ".contains(&br));
        assert!(!"/usr/games/scummvm --path=/g/br2 bladerunner:bladerunner ".contains(&br));
        let sonic = running_marker(&cfg, &ExternalGame { rom: Some("/g/Sonic/disc.gdi".into()), ..game(Kind::Dreamcast, "/g/Sonic") });
        assert!("flatpak run org.flycast.flycast /g/sonic/disc.gdi ".contains(&sonic));
        assert!(!"flycast /g/sonic/disc.gdi.bak ".contains(&sonic));
    }

    #[test]
    fn recognizes_image_extensions() {
        let dreamcast = ["gdi".to_string(), "cdi".to_string()];
        assert!(has_extension(Path::new("/g/Crazy Taxi.CDI"), &dreamcast));
        assert!(has_extension(Path::new("/g/Shenmue/disc.gdi"), &dreamcast));
        assert!(!has_extension(Path::new("/g/game.iso"), &dreamcast));
    }

    #[test]
    fn keeps_settings_saved_by_earlier_versions() {
        let json = r##"{"id":"s","name":"S","kind":"dreamcast","path":"/g","accent":"#000"}"##;
        let game: ExternalGame = serde_json::from_str(json).unwrap();
        assert_eq!(game.settings, None);
        let off: ExternalGame = serde_json::from_str(&json.replace("}", r#","flycast":{}}"#)).unwrap();
        assert_eq!(off.settings, Some(BTreeMap::new()));
        let emu = emulator::get(Kind::Dreamcast).unwrap();
        assert_eq!(emu.settings(&game), emu.default_settings());
        assert!(emu.settings(&off).is_empty());
    }

    #[test]
    fn validates_env_names() {
        assert!(valid_env_name("WINEDLLOVERRIDES"));
        assert!(valid_env_name("_X1"));
        assert!(!valid_env_name("1X"));
        assert!(!valid_env_name("A-B"));
        assert!(!valid_env_name(""));
    }

    #[test]
    fn slugs_names() {
        assert_eq!(slug("Blade Runner (Windows/Spanish)"), "blade-runner-windows-spanish");
        assert_eq!(slug("¡!"), "juego");
    }
}
