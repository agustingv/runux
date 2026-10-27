use crate::config::{self, Config, Runner};
use crate::proton::{self, ProtonBuild};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

pub const BATTLENET_DIR: &str = r"C:\Program Files (x86)\Battle.net";

#[derive(Clone, Serialize)]
pub struct LogLine {
    pub task: String,
    pub line: String,
}

#[derive(Clone, Serialize)]
pub struct TaskFinished {
    pub task: String,
    pub success: bool,
}

pub fn log(app: &AppHandle, task: &str, line: impl Into<String>) {
    let _ = app.emit(
        "log",
        LogLine {
            task: task.into(),
            line: line.into(),
        },
    );
}

fn resolve_proton(cfg: &Config) -> Result<ProtonBuild, String> {
    let build = proton::resolve(cfg.proton_path.as_deref())
        .ok_or("No se encontró ninguna versión de Proton. Descarga GE-Proton en Ajustes o elige Wine.")?;
    // Outside its runtime Proton needs Steam's 32-bit host libraries, and
    // fails cryptically without them (`/lib/ld-linux.so.2: could not open`).
    match &build.missing_runtime {
        Some(rt) => Err(format!("{} necesita {rt}, que no está instalado. Descárgalo desde Ajustes.", build.name)),
        None => Ok(build),
    }
}

/// A command that runs a Windows program in the configured prefix; append the
/// program and its arguments. Unix paths to `.exe` files work with both runners.
pub fn command(cfg: &Config) -> Result<Command, String> {
    let mut cmd = match cfg.runner {
        Runner::Wine => {
            let mut cmd = Command::new(&cfg.wine_bin);
            cmd.env("WINEPREFIX", cfg.wine_prefix()).env("WINEARCH", "win64");
            cmd
        }
        Runner::Proton => {
            let build = resolve_proton(cfg)?;
            let proton = build.path.join("proton");
            // `run`, not `waitforexitandrun`: the latter blocks until the prefix's
            // wineserver exits, so a game could never start while Battle.net is open.
            let mut cmd = match &build.runtime {
                Some(rt) => {
                    let mut cmd = Command::new(rt.join("_v2-entry-point"));
                    cmd.args(["--verb=run", "--"]).arg(proton).arg("run");
                    cmd
                }
                None => {
                    let mut cmd = Command::new(proton);
                    cmd.arg("run");
                    cmd
                }
            };
            // Without Steam this is an empty dir of ours; Proton tolerates that.
            let _ = std::fs::create_dir_all(&build.steam_root);
            let mut tool_paths = build.path.into_os_string();
            if let Some(rt) = &build.runtime {
                tool_paths.push(":");
                tool_paths.push(rt);
            }
            cmd.env("STEAM_COMPAT_DATA_PATH", &cfg.proton_data)
                .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &build.steam_root)
                .env("STEAM_COMPAT_TOOL_PATHS", tool_paths)
                .env("SteamAppId", "0")
                .env("SteamGameId", "0")
                // Instead of the home folder; games get their own below it.
                .env("PROTON_LOG_DIR", config::logs_dir());
            cmd
        }
    };
    cmd.env("WINEDEBUG", "-all").stdin(Stdio::null());
    Ok(cmd)
}

/// Proton's log folder for a game, by its task (`ea:<id>`, `ext:<id>`).
pub fn game_log_dir(task: &str) -> PathBuf {
    config::logs_dir().join(task.replace(':', "-"))
}

/// A command for short-lived Wine tools (`reg`…). With Proton it uses the
/// build's wine binary directly: `proton run` doesn't return while other
/// programs (Battle.net, a game) are running in the prefix.
pub fn tool_command(cfg: &Config) -> Result<Command, String> {
    if cfg.runner == Runner::Wine {
        return command(cfg);
    }
    let bin = resolve_proton(cfg)?.path.join("files/bin/wine");
    let mut cmd = Command::new(bin);
    cmd.env("WINEPREFIX", cfg.wine_prefix())
        .env("WINEDEBUG", "-all")
        .stdin(Stdio::null());
    Ok(cmd)
}

/// How long the version of a wine binary is reused: the UI asks for it every
/// few seconds, and each time would start a wine process.
const WINE_INFO_TTL: Duration = Duration::from_secs(30);
/// The wine binary, when it was checked, its version and whether it runs 64-bit programs.
static WINE_INFO: Mutex<Option<(String, Instant, Option<String>, bool)>> = Mutex::new(None);

/// Human-readable runner name, whether it can run Battle.net, and if not, why.
pub async fn runner_info(cfg: &Config) -> (Option<String>, bool, Option<String>) {
    match cfg.runner {
        Runner::Wine => {
            if let Some((bin, at, version, ok)) = WINE_INFO.lock().unwrap_or_else(|e| e.into_inner()).clone()
                && bin == cfg.wine_bin
                && at.elapsed() < WINE_INFO_TTL
            {
                return (version, ok, None);
            }
            let version = version(cfg).await;
            let ok = version.is_some() && supports_win64(cfg);
            *WINE_INFO.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((cfg.wine_bin.clone(), Instant::now(), version.clone(), ok));
            (version, ok, None)
        }
        Runner::Proton => match resolve_proton(cfg) {
            Ok(b) => {
                let label = match &b.version {
                    Some(v) => format!("{} ({v})", b.name),
                    None => b.name,
                };
                (Some(label), true, None)
            }
            Err(e) => (None, false, Some(e)),
        },
    }
}

/// Resolves the wine binary through `$PATH` and symlinks (e.g. Debian alternatives).
fn resolve_wine(cfg: &Config) -> Option<PathBuf> {
    let bin = Path::new(&cfg.wine_bin);
    let found = if bin.components().count() > 1 {
        Some(bin.to_path_buf())
    } else {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|dir| dir.join(bin))
            .find(|p| p.is_file())
    }?;
    Some(std::fs::canonicalize(&found).unwrap_or(found))
}

/// Battle.net is 64-bit only. Detects a wine64 loader (Debian/Ubuntu split
/// packages) or 64-bit PE modules (upstream, WoW64, Proton/GE builds).
fn supports_win64(cfg: &Config) -> bool {
    let Some(bin) = resolve_wine(cfg) else {
        return false;
    };
    let Some(bin_dir) = bin.parent() else {
        return false;
    };
    if bin_dir.join("wine64").is_file() {
        return true;
    }
    let root = bin_dir.parent().unwrap_or(bin_dir);
    ["lib/wine", "lib64/wine", "lib/x86_64-linux-gnu/wine"]
        .iter()
        .any(|lib| root.join(lib).join("x86_64-windows").is_dir())
}

/// `wineserver` that belongs to the configured runner.
pub fn wineserver(cfg: &Config) -> Result<Command, String> {
    if cfg.runner == Runner::Proton {
        let bin = resolve_proton(cfg)?.path.join("files/bin/wineserver");
        let mut cmd = Command::new(bin);
        cmd.env("WINEPREFIX", cfg.wine_prefix()).stdin(Stdio::null());
        return Ok(cmd);
    }
    let sibling = resolve_wine(cfg)
        .and_then(|w| w.parent().map(|d| d.join("wineserver")))
        .filter(|p| p.is_file());
    // Debian/Ubuntu keep wineserver out of $PATH.
    let distro = [
        "/usr/lib/x86_64-linux-gnu/wine/wineserver",
        "/usr/lib/wine/wineserver64",
        "/usr/lib/wine/wineserver",
    ]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file());
    let bin = sibling.or(distro).unwrap_or_else(|| PathBuf::from("wineserver"));
    let mut cmd = Command::new(bin);
    cmd.env("WINEPREFIX", cfg.wine_prefix()).stdin(Stdio::null());
    Ok(cmd)
}

async fn version(cfg: &Config) -> Option<String> {
    let out = Command::new(&cfg.wine_bin).arg("--version").output().await.ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn prefix_exists(cfg: &Config) -> bool {
    cfg.wine_prefix().join("system.reg").is_file()
}

/// Creates the configured prefix, logging under `task`.
pub async fn create_prefix(app: &AppHandle, cfg: &Config, task: &str) -> Result<(), String> {
    let (_, runner_ok, problem) = runner_info(cfg).await;
    if !runner_ok {
        return Err(problem.unwrap_or_else(|| "El runner no está listo: revisa Wine/Proton en Ajustes".into()));
    }
    let prefix = cfg.wine_prefix();
    log(app, task, format!("Creando prefijo en {}", prefix.display()));
    let mut cmd = command(cfg)?;
    match cfg.runner {
        Runner::Wine => {
            std::fs::create_dir_all(&prefix).map_err(|e| e.to_string())?;
            // Skip the Mono/Gecko install prompts: the clients don't need them.
            cmd.args(["wineboot", "--init"])
                .env("WINEDLLOVERRIDES", "mscoree,mshtml=");
        }
        Runner::Proton => {
            // Proton builds the prefix itself (with DXVK/VKD3D) before running anything.
            std::fs::create_dir_all(&cfg.proton_data).map_err(|e| e.to_string())?;
            cmd.arg("wineboot");
        }
    }
    run_logged(app, task, cmd).await?;
    log(app, task, "Prefijo listo");
    Ok(())
}

pub fn battlenet_exe(cfg: &Config) -> PathBuf {
    to_unix_path(cfg, &format!(r"{BATTLENET_DIR}\Battle.net.exe"))
}

pub fn product_db(cfg: &Config) -> PathBuf {
    to_unix_path(cfg, r"C:\ProgramData\Battle.net\Agent\product.db")
}

/// Maps a Windows path (`C:\Foo` or `C:/Foo`) to its location inside the prefix.
pub fn to_unix_path(cfg: &Config, win: &str) -> PathBuf {
    let win = win.replace('\\', "/");
    let bytes = win.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let drive = format!("{}:", (bytes[0] as char).to_ascii_lowercase());
        let rest = win[2..].trim_start_matches('/');
        let base = cfg.wine_prefix().join("dosdevices").join(drive);
        if rest.is_empty() { base } else { base.join(rest) }
    } else {
        PathBuf::from(win)
    }
}

/// PIDs and lowercased, slash-normalized command lines of every running process.
fn processes() -> Vec<(u32, String)> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let pid = e.file_name().to_str()?.parse().ok()?;
            let raw = std::fs::read(e.path().join("cmdline")).ok()?;
            let cmdline = String::from_utf8_lossy(&raw)
                .replace('\0', " ")
                .replace('\\', "/")
                .to_lowercase();
            Some((pid, cmdline))
        })
        .collect()
}

/// Runner processes that only wait for a program: `proton run` with its
/// container, and Proton's `steam.exe`. They stay alive while anything else
/// runs in the prefix, so they don't tell whether that program still runs.
fn is_launcher(cmdline: &str) -> bool {
    cmdline.contains("/proton run ")
        || cmdline.contains("/proton waitforexitandrun ")
        || cmdline.starts_with("c:/windows/system32/steam.exe")
}

/// Lowercased, slash-normalized command lines of every running process,
/// leaving out the runner's launchers.
pub fn running_cmdlines() -> Vec<String> {
    processes()
        .into_iter()
        .map(|(_, c)| c)
        .filter(|c| !is_launcher(c))
        .collect()
}

/// How Wine names a Unix path, normalized like `running_cmdlines`: under
/// `C:` when it's inside the prefix, else under `Z:`.
pub fn to_win_path(cfg: &Config, path: &Path) -> String {
    let prefix = cfg.wine_prefix();
    let win = [prefix.join("drive_c"), prefix.join("dosdevices/c:")]
        .iter()
        .find_map(|base| path.strip_prefix(base).ok())
        .map_or_else(|| format!("z:{}", path.display()), |rel| format!("c:/{}", rel.display()));
    normalize_win_path(&win)
}

pub fn normalize_win_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

pub fn battlenet_running(cmdlines: &[String]) -> bool {
    cmdlines.iter().any(|c| c.contains("battle.net.exe"))
}

/// Battle.net, its Agent and the runner wrappers launching them. Games run as
/// their own processes, so they are not included.
fn is_battlenet_process(cmdline: &str) -> bool {
    cmdline.contains("battle.net.exe") || cmdline.contains("/battle.net/agent/")
}

/// Makes Battle.net behave like a game: closing its window exits it instead of
/// hiding it in the tray (which Wine/Proton usually can't show), and it never
/// starts minimized. Battle.net rewrites its config on exit, so this is only
/// applied while it is closed.
pub fn prepare_battlenet_config(cfg: &Config) -> Result<(), String> {
    let Ok(users) = std::fs::read_dir(cfg.wine_prefix().join("drive_c/users")) else {
        return Ok(());
    };
    for user in users.flatten() {
        let path = user.path().join("AppData/Roaming/Battle.net/Battle.net.config");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let Some(client) = json.get_mut("Client").and_then(|c| c.as_object_mut()) else {
            continue;
        };
        let mut changed = false;
        for key in ["HideOnClose", "AutoStartMinimized"] {
            if client.get(key).and_then(|v| v.as_str()) != Some("false") {
                client.insert(key.into(), "false".into());
                changed = true;
            }
        }
        if changed {
            let text = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// PIDs terminated on purpose by `close_processes` or the launcher reaper,
/// and when: their exit is not a failure.
static CLOSED: Mutex<Option<HashMap<u32, Instant>>> = Mutex::new(None);
/// Long enough for a terminated child to exit and be waited for. Older
/// entries are dropped, so a reused PID isn't mistaken for one of them.
const CLOSED_TTL: Duration = Duration::from_secs(60);

fn mark_closed(pids: &[u32]) {
    let mut closed = CLOSED.lock().unwrap_or_else(|e| e.into_inner());
    let closed = closed.get_or_insert_with(HashMap::new);
    closed.retain(|_, at| at.elapsed() < CLOSED_TTL);
    let now = Instant::now();
    closed.extend(pids.iter().map(|&pid| (pid, now)));
}

fn matching_pids(matches: fn(&str) -> bool) -> Vec<u32> {
    processes()
        .into_iter()
        .filter(|(pid, c)| *pid != std::process::id() && matches(c))
        .map(|(pid, _)| pid)
        .collect()
}

fn signal(sig: &str, pids: &[u32]) {
    let _ = std::process::Command::new("kill")
        .arg(sig)
        .args(pids.iter().map(u32::to_string))
        .stderr(Stdio::null())
        .status();
}

/// Terminates every Battle.net process and waits for them to exit, forcing
/// the stragglers after a few seconds. Returns how many were running.
pub async fn close_battlenet() -> usize {
    close_processes(is_battlenet_process).await
}

/// Terminates every process whose normalized command line `matches`, as
/// `close_battlenet` does.
pub async fn close_processes(matches: fn(&str) -> bool) -> usize {
    let pids = matching_pids(matches);
    if pids.is_empty() {
        return 0;
    }
    mark_closed(&pids);
    signal("-TERM", &pids);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let left = matching_pids(matches);
        if left.is_empty() {
            break;
        }
        if Instant::now() > deadline {
            mark_closed(&left);
            signal("-KILL", &left);
            break;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    pids.len()
}

fn was_closed(pid: Option<u32>) -> bool {
    let mut closed = CLOSED.lock().unwrap_or_else(|e| e.into_inner());
    pid.and_then(|pid| closed.as_mut()?.remove(&pid))
        .is_some_and(|at| at.elapsed() < CLOSED_TTL)
}

/// A process as the launcher reaper sees it.
struct Proc {
    pid: u32,
    ppid: u32,
    /// Start time, in clock ticks since boot.
    start: u64,
    /// Normalized like `running_cmdlines`.
    cmdline: String,
    args: Vec<String>,
    /// `STEAM_COMPAT_DATA_PATH` of the process, if any.
    compat_data: Option<PathBuf>,
}

/// Kernel clock ticks per second (`USER_HZ`), 100 on every Linux we run on.
const TICKS_PER_SEC: u64 = 100;
/// Time a launcher gets for its program to show up.
const LAUNCHER_GRACE_SECS: u64 = 30;

fn read_proc(pid: u32) -> Option<Proc> {
    let dir = PathBuf::from(format!("/proc/{pid}"));
    let stat = std::fs::read_to_string(dir.join("stat")).ok()?;
    // Fields after the parenthesized name, which may contain spaces.
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let ppid = fields.get(1)?.parse().ok()?;
    let start = fields.get(19)?.parse().ok()?;
    let raw = std::fs::read(dir.join("cmdline")).ok()?;
    let args: Vec<String> = String::from_utf8_lossy(&raw)
        .split('\0')
        .filter(|a| !a.is_empty())
        .map(String::from)
        .collect();
    let cmdline = args.join(" ").replace('\\', "/").to_lowercase();
    let compat_data = cmdline.starts_with("c:/windows/system32/steam.exe").then(|| {
        let env = std::fs::read(dir.join("environ")).ok()?;
        env.split(|b| *b == 0)
            .find_map(|v| v.strip_prefix(b"STEAM_COMPAT_DATA_PATH="))
            .map(|v| PathBuf::from(String::from_utf8_lossy(v).as_ref()))
    });
    Some(Proc { pid, ppid, start, cmdline, args, compat_data: compat_data.flatten() })
}

fn uptime_ticks() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/uptime").ok()?;
    let secs: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some((secs * TICKS_PER_SEC as f64) as u64)
}

/// A command line as compared by the reaper: no quotes, no `\\?\` prefix.
fn bare_cmdline(cmdline: &str) -> String {
    let c = cmdline.replace('"', "");
    c.strip_prefix("//?/").map(String::from).unwrap_or(c)
}

/// What a Proton `steam.exe` runs, as Wine shows it in the program's own
/// command line: the Windows path of the executable, then its arguments.
fn launcher_target(cfg: &Config, args: &[String]) -> Option<String> {
    let program = args.get(1)?;
    let mut target = if program.starts_with('/') {
        to_win_path(cfg, Path::new(program))
    } else {
        normalize_win_path(program)
    };
    for arg in &args[2..] {
        target.push(' ');
        target.push_str(&arg.replace('\\', "/").to_lowercase());
    }
    Some(bare_cmdline(&target))
}

/// Proton launchers in the prefix whose program has ended: each as the
/// `steam.exe` and the `proton run` chain above it. A chain is left alone
/// while its container holds anything else, such as the prefix's
/// `wineserver` or a program started from it.
fn stale_launchers(cfg: &Config, procs: &[Proc], now: u64) -> Vec<(String, Vec<u32>)> {
    let running: Vec<String> = procs
        .iter()
        .filter(|p| !is_launcher(&p.cmdline))
        .map(|p| bare_cmdline(&p.cmdline))
        .collect();
    let by_pid = |pid: u32| procs.iter().find(|p| p.pid == pid);
    procs
        .iter()
        .filter(|p| p.cmdline.starts_with("c:/windows/system32/steam.exe"))
        .filter(|p| p.compat_data.as_deref() == Some(cfg.proton_data.as_path()))
        .filter(|p| now.saturating_sub(p.start) >= LAUNCHER_GRACE_SECS * TICKS_PER_SEC)
        .filter_map(|steam| {
            let target = launcher_target(cfg, &steam.args)?;
            if running.iter().any(|c| c.starts_with(&target)) {
                return None;
            }
            let mut chain = vec![steam.pid];
            let mut parent = by_pid(steam.ppid);
            while let Some(p) = parent.filter(|p| is_launcher(&p.cmdline)) {
                chain.push(p.pid);
                parent = by_pid(p.ppid);
            }
            let busy = procs.iter().any(|p| chain.contains(&p.ppid) && !chain.contains(&p.pid));
            let name = steam.args.get(1).map(|a| {
                a.rsplit(['/', '\\']).next().unwrap_or(a).to_string()
            });
            (!busy).then(|| (name.unwrap_or_default(), chain))
        })
        .collect()
}

/// Ends Proton launchers left waiting after their program exited (they
/// wait for everything else in the prefix, such as Battle.net). Returns the
/// names of the programs whose launchers were closed.
pub fn reap_launchers(cfg: &Config) -> Vec<String> {
    if cfg.runner != Runner::Proton {
        return Vec::new();
    }
    let Some(now) = uptime_ticks() else {
        return Vec::new();
    };
    let procs: Vec<Proc> = std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| read_proc(e.file_name().to_str()?.parse().ok()?))
        .collect();
    let stale = stale_launchers(cfg, &procs, now);
    for (_, chain) in &stale {
        mark_closed(chain);
        signal("-TERM", chain);
    }
    stale.into_iter().map(|(name, _)| name).collect()
}

fn pipe_to_log<R: AsyncRead + Unpin + Send + 'static>(app: AppHandle, task: String, reader: R) {
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if !line.trim().is_empty() {
                log(&app, &task, line);
            }
        }
    });
}

/// Runs a command to completion, streaming its output to the `log` event.
pub async fn run_logged(app: &AppHandle, task: &str, mut cmd: Command) -> Result<(), String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let program = Path::new(cmd.as_std().get_program()).file_name().unwrap_or_default().to_string_lossy().into_owned();
    let mut child = cmd.spawn().map_err(|e| format!("No se pudo ejecutar {program}: {e}"))?;
    // Output readers are detached: wineserver may inherit the pipes and keep them open.
    if let Some(out) = child.stdout.take() {
        pipe_to_log(app.clone(), task.into(), out);
    }
    if let Some(err) = child.stderr.take() {
        pipe_to_log(app.clone(), task.into(), err);
    }
    let pid = child.id();
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if status.success() || was_closed(pid) {
        Ok(())
    } else {
        Err(format!("{task} terminó con {status}"))
    }
}

/// Starts a long-running command in the background; emits `task-finished` when it exits.
pub fn spawn_logged(app: &AppHandle, task: &str, cmd: Command) {
    let app = app.clone();
    let task = task.to_string();
    tauri::async_runtime::spawn(async move {
        let result = run_logged(&app, &task, cmd).await;
        if let Err(e) = &result {
            log(&app, &task, e.clone());
        }
        let _ = app.emit(
            "task-finished",
            TaskFinished {
                task,
                success: result.is_ok(),
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_windows_paths_into_prefix() {
        let cfg = Config {
            runner: Runner::Wine,
            prefix: "/pfx".into(),
            ..Default::default()
        };
        assert_eq!(
            to_unix_path(&cfg, r"C:\Program Files (x86)\World of Warcraft"),
            PathBuf::from("/pfx/dosdevices/c:/Program Files (x86)/World of Warcraft")
        );
        assert_eq!(
            to_unix_path(&cfg, "D:/Games"),
            PathBuf::from("/pfx/dosdevices/d:/Games")
        );
        assert_eq!(to_unix_path(&cfg, "/home/x"), PathBuf::from("/home/x"));
    }

    #[test]
    fn maps_unix_paths_to_windows() {
        let cfg = Config {
            runner: Runner::Wine,
            prefix: "/pfx".into(),
            ..Default::default()
        };
        assert_eq!(
            to_win_path(&cfg, Path::new("/pfx/drive_c/Program Files (x86)/Ubisoft/XIII/system")),
            "c:/program files (x86)/ubisoft/xiii/system"
        );
        assert_eq!(to_win_path(&cfg, Path::new("/pfx/dosdevices/c:/Games/")), "c:/games");
        assert_eq!(to_win_path(&cfg, Path::new("/home/x/Game")), "z:/home/x/game");
    }

    fn proc(pid: u32, ppid: u32, args: &[&str]) -> Proc {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let cmdline = args.join(" ").replace('\\', "/").to_lowercase();
        let compat_data = cmdline.starts_with("c:/windows/system32/steam.exe").then(|| PathBuf::from("/data"));
        Proc { pid, ppid, start: 0, cmdline, args, compat_data }
    }

    /// A `proton run` container launching `program`, with pids from `base`.
    fn launch(base: u32, program: &[&str]) -> Vec<Proc> {
        let run = format!("/steam/proton/proton run {}", program.join(" "));
        let mut steam = vec![r"c:\windows\system32\steam.exe"];
        steam.extend(program);
        vec![
            proc(base, 1, &["/usr/bin/bwrap", "--", &run]),
            proc(base + 1, base, &["pv-adverb", "--", &run]),
            proc(base + 2, base + 1, &["python3", &run]),
            proc(base + 3, base + 2, &steam),
        ]
    }

    #[test]
    fn reaps_only_launchers_left_waiting() {
        let cfg = Config {
            runner: Runner::Proton,
            proton_data: "/data".into(),
            ..Default::default()
        };
        let bnet = "/data/pfx/dosdevices/c:/Program Files (x86)/Battle.net/Battle.net.exe";
        let mut procs = launch(10, &[bnet]);
        // The first container hosts wineserver and every Windows program.
        procs.push(proc(20, 11, &["/proton/files/bin/wineserver"]));
        procs.push(proc(21, 11, &[r"C:\Program Files (x86)\Battle.net\Battle.net.exe"]));
        procs.push(proc(22, 11, &[r"C:\Games\Alive\Alive.exe"]));
        procs.extend(launch(30, &[bnet, "--exec=launch Fen"]));
        procs.extend(launch(40, &["/data/pfx/drive_c/Games/XIII/XIII.exe"]));
        procs.extend(launch(50, &["/data/pfx/drive_c/Games/Alive/Alive.exe"]));
        let now = LAUNCHER_GRACE_SECS * TICKS_PER_SEC;

        let stale = stale_launchers(&cfg, &procs, now);
        let names: Vec<&str> = stale.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Battle.net.exe", "XIII.exe"]);
        assert_eq!(stale[0].1, [33, 32, 31, 30]);
        // Too young: the program may still be starting.
        assert!(stale_launchers(&cfg, &procs, now - 1).is_empty());
    }

    #[test]
    fn skips_runner_launchers() {
        let proton = "python3 /steam/common/proton - experimental/proton run /pfx/drive_c/games/x.exe";
        let container = "/usr/bin/bwrap --args 27 pv-adverb -- /steam/proton 10.0/proton run c:/x.exe";
        let steam = "c:/windows/system32/steam.exe /pfx/drive_c/games/x.exe";
        assert!(is_launcher(proton) && is_launcher(container) && is_launcher(steam));
        assert!(!is_launcher("c:/games/x.exe"));
        assert!(!is_launcher("c:/program files (x86)/battle.net/battle.net.exe"));
    }
}

