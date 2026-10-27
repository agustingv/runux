//! Games that don't come from Battle.net: run with ScummVM, or as a Windows
//! executable in the prefix. Stored in `~/.config/runux/games.json`.

use crate::config::{self, Config};
use crate::{store, wine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Scummvm,
    Windows,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalGame {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    /// Game directory (the ScummVM path, or the folder holding the executable).
    pub path: PathBuf,
    /// Windows executable, for `Kind::Windows`.
    #[serde(default)]
    pub exe: Option<PathBuf>,
    /// ScummVM target, e.g. `bladerunner:bladerunner`.
    #[serde(default)]
    pub scummvm_id: Option<String>,
    pub accent: String,
    /// Whether `path` was extracted by the launcher (and is removed with the game).
    #[serde(default)]
    pub imported: bool,
    /// Extra environment variables for the game's process, e.g.
    /// `WINEDLLOVERRIDES` or `PROTON_USE_WINED3D`.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalStatus {
    #[serde(flatten)]
    pub game: ExternalGame,
    pub running: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    pub kind: Kind,
    /// Empty to use the detected (ScummVM) or file name.
    pub name: String,
    /// ISO images to extract, or a single game directory (ScummVM).
    pub sources: Vec<PathBuf>,
    /// Windows executable, for `Kind::Windows`.
    pub exe: Option<PathBuf>,
}

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

pub fn load() -> Vec<ExternalGame> {
    store::load(&store_path())
}

fn save(games: &[ExternalGame]) -> Result<(), String> {
    store::save(&store_path(), games)
}

pub fn find(id: &str) -> Result<ExternalGame, String> {
    load()
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("Juego desconocido: {id}"))
}

pub fn list(cfg: &Config, cmdlines: &[String]) -> Vec<ExternalStatus> {
    load()
        .into_iter()
        .map(|game| {
            let running = cmdlines.iter().any(|c| c.contains(&running_marker(cfg, &game)));
            ExternalStatus { game, running }
        })
        .collect()
}

/// What the command line of a running game contains. ScummVM carries the
/// game dir in `--path=`; a Windows game runs from its executable's path as
/// Wine names it. Both end in a separator, so `C:/Games/Alive` doesn't match
/// `C:/Games/Alive2`.
fn running_marker(cfg: &Config, game: &ExternalGame) -> String {
    match game.kind {
        // Command lines end in a space, standing for each argument's NUL.
        Kind::Scummvm => format!("--path={} ", game.path.to_string_lossy().to_lowercase()),
        Kind::Windows => format!("{}/", wine::to_win_path(cfg, &game.path)),
    }
}

/// `scummvm` on `$PATH`, or where Debian/Ubuntu install it (`/usr/games` may be
/// missing from `$PATH` when the app is started from the desktop).
pub fn scummvm_bin() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join("scummvm"))
        .chain([PathBuf::from("/usr/games/scummvm")])
        .find(|p| p.is_file())
}

fn scummvm() -> Result<PathBuf, String> {
    scummvm_bin().ok_or_else(|| "ScummVM no está instalado (sudo apt install scummvm)".into())
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
    while games.iter().any(|g| g.id == id) || config::data_dir().join("games").join(&id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// Parses `scummvm --detect` output: a table whose dashed rule gives the column widths.
fn parse_detect(output: &str) -> Option<(String, String)> {
    let mut lines = output.lines();
    let rule = lines.by_ref().find(|l| l.starts_with("---"))?;
    let widths: Vec<usize> = rule.split(' ').map(str::len).collect();
    let row = lines.find(|l| !l.trim().is_empty())?;
    let id_end = (*widths.first()?).min(row.len());
    let desc_start = (id_end + 1).min(row.len());
    let desc_end = (desc_start + widths.get(1)?).min(row.len());
    let id = row.get(..id_end)?.trim();
    let desc = row.get(desc_start..desc_end).or_else(|| row.get(desc_start..))?.trim();
    (!id.is_empty()).then(|| (id.to_string(), desc.to_string()))
}

async fn detect_scummvm(app: &AppHandle, dir: &Path) -> Result<(String, String), String> {
    let out = Command::new(scummvm()?)
        .arg("--detect")
        .arg(format!("--path={}", dir.display()))
        .output()
        .await
        .map_err(|e| format!("No se pudo ejecutar ScummVM: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        wine::log(app, "import", line);
    }
    parse_detect(&text).ok_or_else(|| "ScummVM no reconoce ningún juego en esos archivos".into())
}

fn emit_progress(app: &AppHandle, done: u64, total: u64) {
    let _ = app.emit(
        "progress",
        serde_json::json!({ "task": "import", "downloaded": done, "total": total }),
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
        wine::log(app, "import", format!("Extrayendo {name} ({}/{})", i + 1, isos.len()));
        let mut cmd = Command::new("bsdtar");
        cmd.arg("-xf").arg(iso).arg("-C").arg(dest);
        wine::run_logged(app, "import", cmd)
            .await
            .map_err(|e| format!("No se pudo extraer {name}: {e}"))?;
        done += size;
        emit_progress(app, done, total);
    }
    // ISO 9660 files come out read-only, which would block removing them later.
    let mut cmd = Command::new("chmod");
    cmd.arg("-R").arg("u+w").arg(dest);
    wine::run_logged(app, "import", cmd).await
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
    if req.kind == Kind::Scummvm {
        scummvm()?;
    }

    let (path, imported) = match (req.kind, isos.is_empty()) {
        (Kind::Windows, _) => {
            let exe = req.exe.as_ref().ok_or("Elige el ejecutable del juego")?;
            let dir = exe.parent().ok_or("Ruta de ejecutable no válida")?;
            (dir.to_path_buf(), false)
        }
        (Kind::Scummvm, true) => match req.sources.as_slice() {
            [dir] if dir.is_dir() => (dir.clone(), false),
            _ => return Err("Elige las imágenes ISO del juego o su carpeta".into()),
        },
        (Kind::Scummvm, false) => {
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
        Kind::Scummvm => match detect_scummvm(app, &path).await {
            Ok((id, desc)) => (Some(id), Some(desc)),
            Err(e) => {
                if imported {
                    remove_dir(&path);
                }
                return Err(e);
            }
        },
        Kind::Windows => (None, None),
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
        scummvm_id,
        imported,
        env: BTreeMap::new(),
    };
    games.push(game.clone());
    save(&games)?;
    wine::log(app, "import", format!("Añadido: {}", game.name));
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

pub fn set_env(id: &str, env: BTreeMap<String, String>) -> Result<ExternalGame, String> {
    check_env(&env)?;
    let _lock = lock();
    let mut games = load();
    let game = games
        .iter_mut()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("Juego desconocido: {id}"))?;
    game.env = env;
    let game = game.clone();
    save(&games)?;
    Ok(game)
}

pub fn launch(app: &AppHandle, cfg: &Config, game: &ExternalGame) -> Result<(), String> {
    let task = format!("ext:{}", game.id);
    wine::log(app, &task, format!("Lanzando {}", game.name));
    let cmd = match game.kind {
        Kind::Scummvm => {
            let bin = scummvm()?;
            let target = game.scummvm_id.as_deref().ok_or("Falta el identificador de ScummVM")?;
            let mut cmd = Command::new(bin);
            cmd.arg(format!("--path={}", game.path.display())).arg(target);
            cmd
        }
        Kind::Windows => {
            let exe = game.exe.as_ref().ok_or("Falta el ejecutable del juego")?;
            let mut cmd = wine::command(cfg)?;
            cmd.arg(exe).current_dir(&game.path);
            cmd
        }
    };
    wine::spawn_game(app, &task, cmd, &game.env);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_detect_table() {
        let out = "\
GameID                         Description                                                Full Path
------------------------------ ---------------------------------------------------------- ---------
bladerunner:bladerunner        Blade Runner (Spanish)                                     /g/br
";
        assert_eq!(
            parse_detect(out),
            Some(("bladerunner:bladerunner".into(), "Blade Runner (Spanish)".into()))
        );
        assert_eq!(parse_detect("WARNING: no games found\n"), None);
    }

    #[test]
    fn running_needs_the_whole_folder_name() {
        let cfg = Config { runner: config::Runner::Wine, prefix: "/pfx".into(), ..Default::default() };
        let game = |kind, path: &str| ExternalGame {
            id: "x".into(),
            name: "X".into(),
            kind,
            path: path.into(),
            exe: None,
            scummvm_id: None,
            accent: String::new(),
            imported: false,
            env: BTreeMap::new(),
        };
        let alive = running_marker(&cfg, &game(Kind::Windows, "/pfx/drive_c/Games/Alive"));
        assert!("c:/games/alive/alive.exe ".contains(&alive));
        assert!(!"c:/games/alive2/alive2.exe ".contains(&alive));
        let br = running_marker(&cfg, &game(Kind::Scummvm, "/g/BR"));
        assert!("/usr/games/scummvm --path=/g/br bladerunner:bladerunner ".contains(&br));
        assert!(!"/usr/games/scummvm --path=/g/br2 bladerunner:bladerunner ".contains(&br));
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
