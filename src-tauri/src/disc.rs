//! Installing Windows games from discs: ISO images (loop-mounted with udisks)
//! or physical DVDs, exposed to the prefix as CD-ROM drives so installers
//! find their discs as they would on Windows. All the ISOs of a game are
//! mounted at once, each in its own drive.

use crate::config::Config;
use crate::wine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::AppHandle;
use tokio::process::Command;

const TASK: &str = "disc";
/// Task of the running installer; its `task-finished` event tells the UI it's done.
pub const INSTALL_TASK: &str = "disc-install";
const DRIVES_KEY: &str = r"HKLM\Software\Wine\Drives";
const REG_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
struct Lsblk {
    blockdevices: Vec<BlockDevice>,
}

#[derive(Deserialize)]
struct BlockDevice {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    label: Option<String>,
    mountpoint: Option<String>,
    /// A number, or a string with older `lsblk` versions.
    #[serde(default)]
    size: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpticalDrive {
    pub device: String,
    pub label: Option<String>,
    /// Whether there is a disc in the drive.
    pub media: bool,
}

struct Mounted {
    device: String,
    root: PathBuf,
    label: Option<String>,
    /// A loop device set up for an ISO: unmounted and deleted on release.
    loop_device: bool,
    /// Mounted by us (not by the desktop), so unmounted on release.
    we_mounted: bool,
    /// Drive letter inside the prefix.
    letter: char,
    /// The drive was created by us, not by Wine, so we remove it.
    own_drive: bool,
}

pub struct Session {
    /// ISO paths, or optical drive devices (`/dev/sr0`).
    discs: Vec<String>,
    /// Every disc is mounted at once, each in its own drive, so installers
    /// asking for the next one can be pointed at it. Aligned with `discs`.
    mounted: Vec<Option<Mounted>>,
    /// Executables in `C:` before installing, to find the ones it adds.
    exes_before: HashSet<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscInfo {
    source: String,
    /// Drive letter inside the prefix, e.g. `E`, while mounted.
    letter: Option<String>,
    label: Option<String>,
    root: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    discs: Vec<DiscInfo>,
    /// Executables on the first disc that look like installers, best guess
    /// first, relative to its root (`setup.exe`, `INSTALL\SETUP.EXE`…).
    installers: Vec<String>,
    /// The prefix's `C:` drive, where games get installed.
    drive_c: PathBuf,
    /// Executables added to `C:` since the discs were mounted, the likeliest
    /// game first.
    installed: Vec<PathBuf>,
}

fn is_device(disc: &str) -> bool {
    disc.starts_with("/dev/")
}

async fn lsblk(device: Option<&str>) -> Result<Vec<BlockDevice>, String> {
    let mut cmd = Command::new("lsblk");
    cmd.args(["-J", "-b", "-o", "PATH,TYPE,LABEL,MOUNTPOINT,SIZE"]);
    if let Some(d) = device {
        cmd.arg(d);
    }
    let out = cmd.output().await.map_err(|e| format!("No se pudo ejecutar lsblk: {e}"))?;
    let parsed: Lsblk = serde_json::from_slice(&out.stdout).map_err(|e| format!("lsblk: {e}"))?;
    Ok(parsed.blockdevices)
}

pub async fn optical_drives() -> Result<Vec<OpticalDrive>, String> {
    Ok(lsblk(None)
        .await?
        .into_iter()
        .filter(|d| d.kind == "rom")
        .map(|d| {
            let size = d.size.as_u64().or_else(|| d.size.as_str()?.parse().ok()).unwrap_or(0);
            OpticalDrive { device: d.path, label: d.label, media: size > 0 }
        })
        .collect())
}

async fn udisksctl(args: &[&str]) -> Result<String, String> {
    // No tty: polkit asks through the desktop's agent if it needs a password.
    let out = Command::new("udisksctl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("No se pudo ejecutar udisksctl (instala udisks2): {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Mount point and volume label of `device`, if mounted.
async fn mounted_at(device: &str) -> Result<Option<(PathBuf, Option<String>)>, String> {
    let dev = lsblk(Some(device)).await?.into_iter().next();
    Ok(dev.and_then(|d| Some((PathBuf::from(d.mountpoint?), d.label))))
}

/// Mounts `device` unless it already is, and returns its mount point and
/// whether we mounted it. `wait` gives the desktop a moment to auto-mount it.
async fn mount_device(device: &str, wait: bool) -> Result<(PathBuf, Option<String>, bool), String> {
    for _ in 0..if wait { 10 } else { 1 } {
        if let Some((root, label)) = mounted_at(device).await? {
            return Ok((root, label, false));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    udisksctl(&["mount", "-b", device])
        .await
        .map_err(|e| format!("No se pudo montar {device}: {e}"))?;
    let (root, label) = mounted_at(device).await?.ok_or_else(|| format!("{device} no quedó montado"))?;
    Ok((root, label, true))
}

async fn mount(disc: &str) -> Result<Mounted, String> {
    if is_device(disc) {
        let (root, label, we_mounted) = mount_device(disc, false).await?;
        return Ok(Mounted { device: disc.into(), root, label, loop_device: false, we_mounted, letter: 'd', own_drive: false });
    }
    // "Mapped file /x/disc.iso as /dev/loop12."
    let out = udisksctl(&["loop-setup", "-r", "-f", disc])
        .await
        .map_err(|e| format!("No se pudo abrir la imagen: {e}"))?;
    let device = out
        .split_whitespace()
        .find(|w| w.starts_with("/dev/"))
        .map(|w| w.trim_end_matches('.').to_string())
        .ok_or_else(|| format!("Respuesta inesperada de udisksctl: {out}"))?;
    match mount_device(&device, true).await {
        Ok((root, label, _)) => Ok(Mounted { device, root, label, loop_device: true, we_mounted: true, letter: 'd', own_drive: false }),
        Err(e) => {
            let _ = udisksctl(&["loop-delete", "-b", &device]).await;
            Err(e)
        }
    }
}

/// Whether the loop device still has its ISO. udisks sets them up with
/// autoclear, so the kernel frees them once unmounted; give it a moment.
fn loop_attached(device: &str) -> bool {
    let Some(name) = Path::new(device).file_name() else {
        return false;
    };
    let backing = Path::new("/sys/block").join(name).join("loop/backing_file");
    for _ in 0..10 {
        if !backing.exists() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

/// Synchronous so it can also run while the app exits.
fn unmount(m: &Mounted) {
    // Never prompt for a password while cleaning up.
    let run = |args: &[&str]| {
        let _ = std::process::Command::new("udisksctl")
            .args(args)
            .arg("--no-user-interaction")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    };
    if m.we_mounted {
        run(&["unmount", "-b", &m.device]);
    }
    // Deleting a loop device that autoclear already freed counts as touching
    // another user's device, which polkit only allows to an administrator.
    if m.loop_device && loop_attached(&m.device) {
        run(&["loop-delete", "-b", &m.device]);
    }
}

fn dosdevices(cfg: &Config) -> PathBuf {
    cfg.wine_prefix().join("dosdevices")
}

/// The drive Wine's mount manager gave the disc. While the prefix is running
/// it maps every disc the desktop mounts (and marks it as a CD-ROM) by itself.
fn wine_drive(cfg: &Config, root: &Path) -> Option<char> {
    std::fs::read_dir(dosdevices(cfg)).ok()?.flatten().find_map(|e| {
        let name = e.file_name().to_string_lossy().to_lowercase();
        let letter = name.strip_suffix(':')?;
        let mut chars = letter.chars();
        let l = chars.next().filter(|c| c.is_ascii_lowercase() && chars.next().is_none())?;
        (std::fs::read_link(e.path()).ok()? == root).then_some(l)
    })
}

/// Points `X:` at the disc's files and `X::` at its device (where Wine reads
/// the volume label from), or removes both.
fn link(cfg: &Config, letter: char, mounted: Option<&Mounted>) -> Result<(), String> {
    let dos = dosdevices(cfg);
    let drive = dos.join(format!("{letter}:"));
    let device = dos.join(format!("{letter}::"));
    let _ = std::fs::remove_file(&drive);
    let _ = std::fs::remove_file(&device);
    if let Some(m) = mounted {
        std::os::unix::fs::symlink(&m.root, &drive).map_err(|e| format!("{}: {e}", drive.display()))?;
        std::os::unix::fs::symlink(&m.device, &device).map_err(|e| format!("{}: {e}", device.display()))?;
    }
    Ok(())
}

fn free_letter(cfg: &Config) -> Option<char> {
    let dos = dosdevices(cfg);
    ('d'..='y').find(|l| {
        [format!("{l}:"), format!("{l}::")]
            .iter()
            .all(|n| dos.join(n).symlink_metadata().is_err())
    })
}

async fn set_drive_type(app: &AppHandle, cfg: &Config, letter: char, cdrom: bool) -> Result<(), String> {
    let value = format!("{letter}:");
    let mut cmd = wine::tool_command(cfg)?;
    if cdrom {
        cmd.args(["reg", "add", DRIVES_KEY, "/v", &value, "/t", "REG_SZ", "/d", "cdrom", "/f"]);
    } else {
        cmd.args(["reg", "delete", DRIVES_KEY, "/v", &value, "/f"]);
    }
    // Never leave the UI waiting on a stuck wine (e.g. a first-run prefix update).
    cmd.kill_on_drop(true);
    tokio::time::timeout(REG_TIMEOUT, wine::run_logged(app, TASK, cmd))
        .await
        .map_err(|_| "Wine no respondió al configurar la unidad de CD".to_string())?
}

/// Gives the mounted disc a drive letter: Wine's own if it maps it, or one we
/// create (and remove on detach) when the prefix isn't running.
async fn attach(app: &AppHandle, cfg: &Config, m: &mut Mounted) -> Result<(), String> {
    for _ in 0..15 {
        if let Some(letter) = wine_drive(cfg, &m.root) {
            m.letter = letter;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let letter = free_letter(cfg).ok_or("No queda ninguna letra de unidad libre en el prefijo")?;
    set_drive_type(app, cfg, letter, true).await?;
    link(cfg, letter, Some(m))?;
    m.letter = letter;
    m.own_drive = true;
    Ok(())
}

/// Removes our drive (not the registry entry) and unmounts; synchronous so it
/// can also run while the app exits.
fn release_mounted(cfg: &Config, m: &Mounted) {
    if m.own_drive {
        let _ = link(cfg, m.letter, None);
    }
    unmount(m);
}

async fn detach(app: &AppHandle, cfg: &Config, m: Mounted) {
    release_mounted(cfg, &m);
    if m.own_drive && set_drive_type(app, cfg, m.letter, false).await.is_err() {
        wine::log(app, TASK, "No se pudo quitar la unidad del registro de Wine");
    }
}

/// ISO images in `dir` (not recursive), for games kept as a folder of discs.
pub fn isos_in(dir: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let isos: Vec<String> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso")))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if isos.is_empty() {
        return Err(format!("No hay imágenes ISO en {}", dir.display()));
    }
    Ok(isos)
}

async fn mount_one(app: &AppHandle, cfg: &Config, disc: &str) -> Result<Mounted, String> {
    let name = Path::new(disc).file_name().map_or(disc.into(), |n| n.to_string_lossy().into_owned());
    wine::log(app, TASK, format!("Montando {name}"));
    let mut mounted = mount(disc).await?;
    if let Err(e) = attach(app, cfg, &mut mounted).await {
        detach(app, cfg, mounted).await;
        return Err(e);
    }
    wine::log(
        app,
        TASK,
        format!(
            "{}: → {} ({})",
            mounted.letter.to_ascii_uppercase(),
            mounted.root.display(),
            mounted.label.as_deref().unwrap_or("sin etiqueta"),
        ),
    );
    Ok(mounted)
}

pub async fn start(app: &AppHandle, cfg: &Config, discs: Vec<String>) -> Result<Session, String> {
    if discs.is_empty() {
        return Err("Elige las imágenes ISO o la unidad de DVD".into());
    }
    if !wine::prefix_exists(cfg) {
        return Err("Primero hay que crear el prefijo".into());
    }
    let exes_before = exes_in(&cfg.wine_prefix().join("drive_c"));
    let mut session = Session { mounted: Vec::new(), discs, exes_before };
    for disc in session.discs.clone() {
        match mount_one(app, cfg, &disc).await {
            Ok(m) => session.mounted.push(Some(m)),
            Err(e) => {
                end(app, cfg, session).await;
                return Err(e);
            }
        }
    }
    Ok(session)
}

/// Mounts disc `index` again. For a physical drive this re-reads it, after
/// the user has swapped discs.
pub async fn remount(app: &AppHandle, cfg: &Config, session: &mut Session, index: usize) -> Result<(), String> {
    let disc = session.discs.get(index).ok_or("Disco no válido")?.clone();
    if let Some(m) = session.mounted[index].take() {
        detach(app, cfg, m).await;
    }
    session.mounted[index] = Some(mount_one(app, cfg, &disc).await?);
    Ok(())
}

/// Unmounts disc `index`; a physical one is also ejected so the next can go in.
pub async fn eject(app: &AppHandle, cfg: &Config, session: &mut Session, index: usize) -> Result<(), String> {
    let Some(m) = session.mounted.get_mut(index).and_then(Option::take) else {
        return Ok(());
    };
    let (loop_device, device) = (m.loop_device, m.device.clone());
    detach(app, cfg, m).await;
    if loop_device {
        return Ok(());
    }
    // Desktop auto-mounts would keep the tray locked: unmount them too.
    udisksctl(&["unmount", "-b", &device]).await.ok();
    let mut cmd = Command::new("eject");
    cmd.arg(&device);
    wine::run_logged(app, TASK, cmd).await
}

/// Undoes everything but the registry entries; safe to call while exiting.
pub fn release(cfg: &Config, session: &Session) {
    for m in session.mounted.iter().flatten() {
        release_mounted(cfg, m);
    }
}

pub async fn end(app: &AppHandle, cfg: &Config, session: Session) {
    for m in session.mounted.into_iter().flatten() {
        detach(app, cfg, m).await;
    }
    wine::log(app, TASK, "Discos desmontados");
}

pub fn info(cfg: &Config, session: &Session) -> SessionInfo {
    let discs = session
        .discs
        .iter()
        .zip(&session.mounted)
        .map(|(source, m)| DiscInfo {
            source: source.clone(),
            letter: m.as_ref().map(|m| m.letter.to_ascii_uppercase().to_string()),
            label: m.as_ref().and_then(|m| m.label.clone()),
            root: m.as_ref().map(|m| m.root.clone()),
        })
        .collect();
    SessionInfo {
        discs,
        installers: first_disc(session).map(|m| installers(&m.root)).unwrap_or_default(),
        drive_c: cfg.wine_prefix().join("drive_c"),
        installed: installed(cfg, session),
    }
}

/// Folders of `C:` that hold Windows itself or per-user data, not games.
const SYSTEM_DIRS: &[&str] = &["windows", "users", "programdata"];

/// Every `.exe` under `C:`, outside the system folders.
fn exes_in(drive_c: &Path) -> HashSet<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut HashSet<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let Ok(kind) = e.file_type() else {
                continue;
            };
            let path = e.path();
            if kind.is_dir() && depth > 0 {
                walk(&path, depth - 1, out);
            } else if kind.is_file() && path.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
                out.insert(path);
            }
        }
    }
    let mut out = HashSet::new();
    let Ok(entries) = std::fs::read_dir(drive_c) else {
        return out;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if e.file_type().is_ok_and(|t| t.is_dir()) && !SYSTEM_DIRS.contains(&name.as_str()) {
            walk(&e.path(), 6, &mut out);
        }
    }
    out
}

/// Words in the names of executables that come with a game but aren't it.
const NOT_THE_GAME: &[&str] = &[
    "unins", "setup", "install", "regist", "schedule", "update", "patch", "crash", "report",
    "config", "setting", "server", "ucc", "trailer", "readme", "redist", "vcredist", "directx", "dxsetup",
    "activation", "cleanup", "touchup", "eacore", "proxy",
];

/// Lower is likelier the game: not a helper, named like its folder
/// (`XIII\system\XIII.exe`), then the biggest.
pub(crate) fn game_rank(path: &Path) -> (bool, bool, std::cmp::Reverse<u64>) {
    // Letters and digits only, so `MassEffect3.exe` matches `Mass Effect 3`.
    let squash = |s: &str| -> String { s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase() };
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
    let helper = NOT_THE_GAME.iter().any(|w| stem.contains(w));
    let named_like_folder = path
        .ancestors()
        .skip(1)
        .take(3)
        .any(|a| a.file_name().is_some_and(|n| squash(&n.to_string_lossy()) == squash(&stem)));
    let size = std::fs::metadata(path).map_or(0, |m| m.len());
    (helper, !named_like_folder, std::cmp::Reverse(size))
}

/// The likeliest game among the `.exe` files in a game's folder, outside
/// an EA game's own installer.
pub(crate) fn guess_exe(dir: &Path) -> Option<PathBuf> {
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
    exes.into_iter().min_by_key(|p| game_rank(p))
}

fn installed(cfg: &Config, session: &Session) -> Vec<PathBuf> {
    let mut new: Vec<PathBuf> = exes_in(&cfg.wine_prefix().join("drive_c"))
        .into_iter()
        .filter(|p| !session.exes_before.contains(p))
        .collect();
    new.sort_by_cached_key(|p| game_rank(p));
    new
}

fn first_disc(session: &Session) -> Option<&Mounted> {
    session.mounted.first()?.as_ref()
}

pub fn run_installer(app: &AppHandle, cfg: &Config, session: &Session, installer: &str) -> Result<(), String> {
    let m = first_disc(session).ok_or("El primer disco no está montado")?;
    let path = format!(r"{}:\{}", m.letter.to_ascii_uppercase(), installer.trim_start_matches('\\'));
    wine::log(app, INSTALL_TASK, format!("Ejecutando {path}. Sigue los pasos en su ventana."));
    // The plain wine binary, so the task ends when the installer does
    // (`proton run` waits for everything else running in the prefix).
    let mut cmd = wine::tool_command(cfg)?;
    // Run it from the drive letter, as installers expect to find their files
    // there; the Unix working directory would keep the disc busy.
    cmd.arg(path).current_dir(cfg.wine_prefix());
    wine::spawn_logged(app, INSTALL_TASK, cmd);
    Ok(())
}

/// Resolves a `\`-separated path case-insensitively, as ISO 9660 names are
/// usually upper case while `autorun.inf` may not be.
pub(crate) fn find_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|p| !p.is_empty()) {
        let entry = std::fs::read_dir(&path)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        path = entry.path();
    }
    Some(path)
}

/// The program in `autorun.inf`'s `open=`, without its arguments.
fn autorun_target(inf: &str) -> Option<String> {
    let mut in_section = false;
    for line in inf.lines().map(str::trim) {
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[autorun]");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !in_section || !key.trim().eq_ignore_ascii_case("open") {
            continue;
        }
        let value = value.trim();
        let program = match value.strip_prefix('"') {
            Some(rest) => rest.split('"').next()?,
            None => value.split_whitespace().next()?,
        };
        return Some(program.replace('/', "\\").trim_start_matches(['.', '\\']).to_string());
    }
    None
}

fn installers(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let autorun = find_ci(root, "autorun.inf")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| autorun_target(&String::from_utf8_lossy(&b)));
    if let Some(target) = autorun.filter(|t| find_ci(root, t).is_some_and(|p| p.is_file())) {
        found.push(target);
    }
    let mut exes: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().ends_with(".exe"))
        .collect();
    let rank = |n: &str| match n.to_lowercase().as_str() {
        "setup.exe" => 0,
        "install.exe" => 1,
        _ => 2,
    };
    exes.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.to_lowercase().cmp(&b.to_lowercase())));
    for exe in exes {
        if !found.iter().any(|f| f.eq_ignore_ascii_case(&exe)) {
            found.push(exe);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_the_game_first() {
        let dir = std::env::temp_dir().join(format!("runux-rank-{}", std::process::id()));
        let sys = dir.join("Ubisoft/XIII/system");
        std::fs::create_dir_all(&sys).unwrap();
        let files = [("XIIIEd.exe", 900), ("XIII.exe", 100), ("UCC.exe", 50), ("unins000.exe", 2000)];
        for (name, size) in files {
            std::fs::write(sys.join(name), vec![0u8; size]).unwrap();
        }
        let mut exes: Vec<PathBuf> = files.iter().map(|(n, _)| sys.join(n)).collect();
        exes.sort_by_cached_key(|p| game_rank(p));
        let names: Vec<_> = exes.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, ["XIII.exe", "XIIIEd.exe", "unins000.exe", "UCC.exe"]);
    }

    #[test]
    fn parses_autorun() {
        assert_eq!(
            autorun_target("[AutoRun]\r\nopen=SETUP.EXE /autorun\r\nicon=game.ico\r\n"),
            Some("SETUP.EXE".into())
        );
        assert_eq!(
            autorun_target("[autorun]\nOpen = \"Install Files/My Setup.exe\" -x\n"),
            Some(r"Install Files\My Setup.exe".into())
        );
        assert_eq!(autorun_target("[other]\nopen=x.exe\n"), None);
    }
}
