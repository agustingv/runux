mod battlenet;
mod config;
mod covers;
mod disc;
mod download;
mod dxvk;
mod ea;
mod emulator;
mod exeicon;
mod external;
mod help;
mod iso9660;
mod host;
mod productdb;
mod proton;
mod store;
mod task;
mod wine;

use config::{Config, Runner};
use battlenet::Game;
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, RunEvent, State};
use ts_rs::TS;

struct AppState {
    config: Mutex<Config>,
    /// Disc mounted in the prefix to install a game from.
    disc: tokio::sync::Mutex<Option<disc::Session>>,
}

impl AppState {
    fn config(&self) -> Config {
        self.config.lock().unwrap().clone()
    }
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct Status {
    runner: Runner,
    /// Wine version or Proton build name.
    runner_label: Option<String>,
    /// Whether the runner can run Battle.net (64-bit wine, or a Proton build found).
    runner_ok: bool,
    /// Why the runner can't be used, when known.
    runner_problem: Option<String>,
    prefix_exists: bool,
    dxvk_version: Option<String>,
    battlenet_installed: bool,
    battlenet_running: bool,
    /// The emulators, installed or not.
    emulators: Vec<emulator::EmulatorInfo>,
    /// The help page for Windows games.
    windows_help: Vec<help::HelpSection>,
    ea_installed: bool,
    ea_running: bool,
    /// The EA app's in-game overlay; `None` before anyone signs in.
    ea_overlay: Option<bool>,
    /// Set when the 32-bit graphics drivers that Battle.net and the EA app need are missing.
    missing_32bit: Option<host::Missing32Bit>,
    install_hints: host::InstallHints,
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Config {
    state.config()
}

#[tauri::command]
fn set_config(state: State<'_, AppState>, config: Config) -> Result<(), String> {
    let mut current = state.config.lock().unwrap();
    config.save()?;
    *current = config;
    Ok(())
}

/// Everything the UI shows, which it asks for every few seconds.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct Snapshot {
    status: Status,
    games: Vec<Game>,
    external: Vec<external::ExternalStatus>,
    ea_games: Vec<ea::EaGame>,
}

#[tauri::command]
async fn snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let cfg = state.config();
    // Read once: every list needs it to tell what's running.
    let cmdlines = wine::running_cmdlines();
    let status = status(&cfg, &cmdlines).await;
    let games = if status.battlenet_installed { battlenet::games(&cfg, &cmdlines)? } else { Vec::new() };
    let ea_games = if status.ea_installed { ea::games(&cfg.ea(), &cmdlines) } else { Vec::new() };
    Ok(Snapshot { games, external: external::list(&cfg, &cmdlines), ea_games, status })
}

async fn status(cfg: &Config, cmdlines: &[String]) -> Status {
    let (runner_label, runner_ok, runner_problem) = wine::runner_info(cfg).await;
    Status {
        runner: cfg.runner,
        runner_label,
        runner_ok,
        runner_problem,
        prefix_exists: wine::prefix_exists(cfg),
        dxvk_version: dxvk::installed_version(cfg),
        battlenet_installed: battlenet::installed(cfg),
        battlenet_running: battlenet::running(cmdlines),
        emulators: emulator::infos(),
        windows_help: help::windows(cfg),
        ea_installed: ea::installed(&cfg.ea()),
        ea_running: ea::running(cmdlines),
        ea_overlay: ea::overlay_enabled(&cfg.ea()),
        missing_32bit: host::missing_32bit_graphics(),
        install_hints: host::install_hints(),
    }
}

#[tauri::command]
async fn create_prefix(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    wine::create_prefix(&app, &state.config(), task::Task::Prefix.name()).await
}

#[tauri::command]
async fn install_dxvk(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let cfg = state.config();
    if cfg.runner == Runner::Proton {
        return Err("Proton ya incluye DXVK".into());
    }
    wine::require_prefix(&cfg)?;
    dxvk::install(&app, &cfg).await
}

#[tauri::command]
async fn install_battlenet(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    battlenet::install(&app, &state.config()).await
}

#[tauri::command]
fn launch_battlenet(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    battlenet::launch(&app, &state.config())
}

#[tauri::command]
async fn close_battlenet(app: AppHandle) -> Result<(), String> {
    match battlenet::close().await {
        0 => wine::log(&app, battlenet::TASK, "Battle.net no estaba abierto"),
        n => wine::log(&app, battlenet::TASK, format!("Battle.net cerrado ({n} procesos)")),
    }
    Ok(())
}

#[tauri::command]
async fn install_ea(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    ea::install(&app, &state.config().ea()).await
}

#[tauri::command]
fn launch_ea(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    ea::launch(&app, &state.config().ea())
}

#[tauri::command]
async fn close_ea(app: AppHandle) -> Result<(), String> {
    match ea::close().await {
        0 => wine::log(&app, ea::TASK, "La EA app no estaba abierta"),
        n => wine::log(&app, ea::TASK, format!("EA app cerrada ({n} procesos)")),
    }
    Ok(())
}

#[tauri::command]
async fn launch_ea_game(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let cfg = state.config().ea();
    let game = ea::find_game(&cfg, &id, &[])?;
    ea::launch_game(&app, &cfg, &game).await
}

#[tauri::command]
async fn uninstall_ea_game(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let cfg = state.config().ea();
    // Running games can't be uninstalled.
    let game = ea::find_game(&cfg, &id, &wine::running_cmdlines())?;
    ea::uninstall_game(&app, &cfg, &game).await
}

#[tauri::command]
fn set_ea_overlay(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    ea::set_overlay(&state.config().ea(), enabled)
}

#[tauri::command]
fn set_ea_game_env(id: String, env: std::collections::BTreeMap<String, String>) -> Result<(), String> {
    ea::set_env(&id, env)
}

#[tauri::command]
fn open_ea_game_folder(state: State<'_, AppState>, id: String) -> Result<(), String> {
    xdg_open(&ea::find_game(&state.config().ea(), &id, &[])?.path)
}

#[tauri::command]
fn open_ea_prefix(state: State<'_, AppState>) -> Result<(), String> {
    xdg_open(&state.config().ea().wine_prefix())
}

#[tauri::command]
fn launch_game(app: AppHandle, state: State<'_, AppState>, uid: String) -> Result<(), String> {
    let cfg = state.config();
    battlenet::launch_game(&app, &cfg, &battlenet::find_game(&cfg, &uid, &[])?)
}

#[tauri::command]
fn uninstall_game(app: AppHandle, state: State<'_, AppState>, uid: String) -> Result<(), String> {
    let cfg = state.config();
    // Running games can't be uninstalled.
    battlenet::uninstall(&app, &cfg, &battlenet::find_game(&cfg, &uid, &wine::running_cmdlines())?)
}

#[tauri::command]
async fn stop_wine(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let cfg = state.config();
    let mut cmd = wine::wineserver(&cfg)?;
    cmd.arg("-k");
    // wineserver -k exits non-zero when nothing is running in the prefix.
    if wine::run_logged(&app, "wineserver", cmd).await.is_err() {
        wine::log(&app, "wineserver", "No había procesos de Wine en marcha");
    }
    Ok(())
}

fn xdg_open(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("{} no existe", path.display()));
    }
    let mut child = std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    // Reaped, or it would linger as a zombie until the app exits.
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[tauri::command]
fn open_prefix(state: State<'_, AppState>) -> Result<(), String> {
    xdg_open(&state.config().wine_prefix())
}

/// For Settings, which shows what is installed right now.
#[tauri::command]
fn list_protons() -> Vec<proton::ProtonBuild> {
    proton::forget_scan();
    proton::discover()
}

#[tauri::command]
async fn install_ge_proton(app: AppHandle) -> Result<proton::GeInstall, String> {
    let result = proton::install_ge(&app).await;
    proton::forget_scan();
    result
}

#[tauri::command]
fn remove_proton(state: State<'_, AppState>, path: String) -> Result<(), String> {
    proton::remove_downloaded(&state.config(), Path::new(&path))
}

/// Stops the download of a task, such as GE-Proton's.
#[tauri::command]
fn cancel_download(task: String) {
    download::cancel(&task);
}

fn data_url(mime: &str, data: &[u8]) -> String {
    use base64::Engine;
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(data))
}

/// Data URL of the picture an emulator game's disc shows behind it in the
/// console's menu (`PIC1.PNG`), if it has one. `key` as for `game_icon`.
#[tauri::command]
async fn game_background(key: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        let game = external::find(key.strip_prefix("ext:")?).ok()?;
        let png = emulator::get(game.kind)?.art(game.rom.as_deref()?, emulator::Art::Background)?;
        Some(data_url("image/png", &png))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Data URL of a game's icon: the one in its executable or, for a game run
/// by an emulator, the one on its disc or the box art the player downloaded.
/// `key` is the game's key in the sidebar: a Battle.net uid, `ea:<id>` or
/// `ext:<id>`.
#[tauri::command]
async fn game_icon(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    let cfg = state.config();
    tokio::task::spawn_blocking(move || {
        let exe = if let Some(id) = key.strip_prefix("ea:") {
            ea::find_game(&cfg.ea(), id, &[]).ok()?.exe?
        } else if let Some(id) = key.strip_prefix("ext:") {
            let game = external::find(id).ok()?;
            if let Some(emu) = emulator::get(game.kind) {
                let rom = game.rom.as_deref()?;
                if let Some(png) = emu.art(rom, emulator::Art::Icon) {
                    return Some(data_url("image/png", &png));
                }
                return covers::cached(&emu.cover(rom)?).map(|jpeg| data_url("image/jpeg", &jpeg));
            }
            game.exe?
        } else {
            // Battle.net doesn't say which executable is the game's.
            let game = battlenet::find_game(&cfg, &key, &[]).ok()?;
            disc::guess_exe(Path::new(&game.local_path))?
        };
        let icon = exeicon::read(&exe)?;
        Some(data_url(icon.mime, &icon.data))
    })
    .await
    .map_err(|e| e.to_string())
}

/// The box art an emulator game can have downloaded.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct CoverInfo {
    /// The game's serial, the only thing sent to download it.
    serial: String,
    downloaded: bool,
}

/// The cover of the external game `id`, for the systems that have them.
fn external_cover(id: &str) -> Result<Option<emulator::Cover>, String> {
    let game = external::find(id)?;
    Ok(emulator::get(game.kind).zip(game.rom.as_deref()).and_then(|(emu, rom)| emu.cover(rom)))
}

/// Whether the external game `id` can have box art downloaded, and whether
/// it has. `None` for games whose discs carry their pictures, or no serial.
#[tauri::command]
async fn cover_info(id: String) -> Result<Option<CoverInfo>, String> {
    tokio::task::spawn_blocking(move || {
        Ok(external_cover(&id)?.map(|c| CoverInfo { downloaded: covers::is_cached(&c), serial: c.serial }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Downloads the box art of the external game `id`, when the player asks.
#[tauri::command]
async fn download_cover(id: String) -> Result<(), String> {
    let cover = tokio::task::spawn_blocking(move || external_cover(&id)).await.map_err(|e| e.to_string())??;
    covers::download(&cover.ok_or("Este juego no tiene carátula que descargar")?).await
}

#[tauri::command]
fn remove_cover(id: String) -> Result<(), String> {
    external_cover(&id)?.map_or(Ok(()), |c| covers::remove(&c))
}

#[tauri::command]
fn open_game_folder(state: State<'_, AppState>, uid: String) -> Result<(), String> {
    xdg_open(Path::new(&battlenet::find_game(&state.config(), &uid, &[])?.local_path))
}

/// Opens the folder of a game's Proton log; `key` is its task (`ea:<id>`, `ext:<id>`).
#[tauri::command]
fn open_game_logs(key: String) -> Result<(), String> {
    if !(key.starts_with("ea:") || key.starts_with("ext:")) || key.contains('/') {
        return Err(format!("Juego desconocido: {key}"));
    }
    let dir = wine::game_log_dir(&key);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    xdg_open(&dir)
}

#[tauri::command]
async fn add_external_game(app: AppHandle, request: external::AddRequest) -> Result<external::ExternalGame, String> {
    external::add(&app, request).await
}

#[tauri::command]
fn remove_external_game(id: String) -> Result<(), String> {
    external::remove(&id)
}

#[tauri::command]
fn launch_external_game(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    external::launch(&app, &state.config(), &external::find(&id)?)
}

#[tauri::command]
fn set_external_env(
    id: String,
    env: std::collections::BTreeMap<String, String>,
) -> Result<external::ExternalGame, String> {
    external::set_env(&id, env)
}

#[tauri::command]
fn set_external_settings(
    id: String,
    settings: std::collections::BTreeMap<String, String>,
) -> Result<external::ExternalGame, String> {
    external::set_settings(&id, settings)
}

/// Opens an emulator on its own, to set it up.
#[tauri::command]
fn open_emulator(app: AppHandle, kind: external::Kind) -> Result<(), String> {
    let emu = emulator::get(kind).ok_or("No es un emulador")?;
    wine::spawn_logged(&app, &format!("emu:{kind:?}"), emu.open()?);
    Ok(())
}

#[tauri::command]
fn open_external_folder(id: String) -> Result<(), String> {
    xdg_open(&external::find(&id)?.path)
}

#[tauri::command]
async fn list_optical_drives() -> Result<Vec<disc::OpticalDrive>, String> {
    disc::optical_drives().await
}

#[tauri::command]
async fn start_disc_session(
    app: AppHandle,
    state: State<'_, AppState>,
    discs: Vec<String>,
) -> Result<disc::SessionInfo, String> {
    let cfg = state.config();
    let mut session = state.disc.lock().await;
    if let Some(old) = session.take() {
        disc::end(&app, &cfg, old).await;
    }
    let new = disc::start(&app, &cfg, discs).await?;
    Ok(disc::info(&cfg, session.insert(new)))
}

#[tauri::command]
async fn change_disc(app: AppHandle, state: State<'_, AppState>, index: usize) -> Result<disc::SessionInfo, String> {
    let cfg = state.config();
    let mut guard = state.disc.lock().await;
    let session = guard.as_mut().ok_or("No hay ningún disco en uso")?;
    let result = disc::remount(&app, &cfg, session, index).await;
    result.map(|_| disc::info(&cfg, session))
}

#[tauri::command]
async fn eject_disc(app: AppHandle, state: State<'_, AppState>, index: usize) -> Result<disc::SessionInfo, String> {
    let cfg = state.config();
    let mut guard = state.disc.lock().await;
    let session = guard.as_mut().ok_or("No hay ningún disco en uso")?;
    disc::eject(&app, &cfg, session, index).await?;
    Ok(disc::info(&cfg, session))
}

#[tauri::command]
fn list_isos(dir: String) -> Result<Vec<String>, String> {
    disc::isos_in(Path::new(&dir))
}

#[tauri::command]
async fn disc_session_info(state: State<'_, AppState>) -> Result<Option<disc::SessionInfo>, String> {
    let cfg = state.config();
    Ok(state.disc.lock().await.as_ref().map(|s| disc::info(&cfg, s)))
}

#[tauri::command]
async fn run_disc_installer(app: AppHandle, state: State<'_, AppState>, installer: String) -> Result<(), String> {
    let cfg = state.config();
    let guard = state.disc.lock().await;
    let session = guard.as_ref().ok_or("No hay ningún disco en uso")?;
    disc::run_installer(&app, &cfg, session, &installer)
}

#[tauri::command]
async fn end_disc_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let session = state.disc.lock().await.take();
    if let Some(session) = session {
        disc::end(&app, &state.config(), session).await;
    }
    Ok(())
}

/// Periodically closes Proton launchers left waiting after their program
/// exited, so they don't pile up while Battle.net keeps the prefix running.
async fn reap_launchers(app: AppHandle) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tick.tick().await;
        let cfg = app.state::<AppState>().config();
        let reaped = [wine::reap_launchers(&cfg), wine::reap_launchers(&cfg.ea())].concat();
        for name in reaped {
            wine::log(&app, "proton", format!("Cerrado el lanzador de {name}, que ya había terminado"));
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    config::migrate_from_btn();
    proton::remove_old_client_dir();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            config: Mutex::new(Config::load()),
            disc: tokio::sync::Mutex::new(None),
        })
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(reap_launchers(handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            snapshot,
            create_prefix,
            install_dxvk,
            install_battlenet,
            launch_battlenet,
            close_battlenet,
            install_ea,
            launch_ea,
            close_ea,
            open_ea_prefix,
            launch_ea_game,
            set_ea_game_env,
            set_ea_overlay,
            uninstall_ea_game,
            open_ea_game_folder,
            launch_game,
            uninstall_game,
            stop_wine,
            open_prefix,
            open_game_folder,
            game_icon,
            game_background,
            cover_info,
            download_cover,
            remove_cover,
            open_game_logs,
            list_protons,
            install_ge_proton,
            remove_proton,
            cancel_download,
            add_external_game,
            remove_external_game,
            launch_external_game,
            open_external_folder,
            set_external_env,
            set_external_settings,
            open_emulator,
            list_optical_drives,
            list_isos,
            start_disc_session,
            change_disc,
            eject_disc,
            disc_session_info,
            run_disc_installer,
            end_disc_session,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // Don't leave the ISO mounted and mapped in the prefix. A disc
            // operation may be finishing: wait a little for it, not forever.
            if let RunEvent::Exit = event {
                let state = app.state::<AppState>();
                for _ in 0..30 {
                    if let Ok(mut session) = state.disc.try_lock() {
                        if let Some(session) = session.take() {
                            disc::release(&state.config(), &session);
                        }
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        });
}
