mod config;
mod disc;
mod download;
mod dxvk;
mod ea;
mod exeicon;
mod external;
mod games;
mod host;
mod productdb;
mod proton;
mod store;
mod wine;

use config::{Config, Runner};
use games::Game;
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, RunEvent, State};

const INSTALLER_URL: &str =
    "https://downloader.battle.net/download/getInstaller?os=win&installer=Battle.net-Setup.exe";

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
    scummvm_installed: bool,
    ea_installed: bool,
    ea_running: bool,
    /// The EA app's in-game overlay; `None` before anyone signs in.
    ea_overlay: Option<bool>,
    /// Set when the 32-bit graphics drivers that Battle.net and the EA app need are missing.
    missing_32bit: Option<host::Missing32Bit>,
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
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
    let games = if status.battlenet_installed { battlenet_games(&cfg, &cmdlines)? } else { Vec::new() };
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
        battlenet_installed: wine::battlenet_exe(cfg).is_file(),
        battlenet_running: wine::battlenet_running(cmdlines),
        scummvm_installed: external::scummvm_bin().is_some(),
        ea_installed: ea::installed(&cfg.ea()),
        ea_running: ea::running(cmdlines),
        ea_overlay: ea::overlay_enabled(&cfg.ea()),
        missing_32bit: host::missing_32bit_graphics(),
    }
}

#[tauri::command]
async fn create_prefix(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    wine::create_prefix(&app, &state.config(), "prefix").await
}

#[tauri::command]
async fn install_dxvk(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let cfg = state.config();
    if cfg.runner == Runner::Proton {
        return Err("Proton ya incluye DXVK".into());
    }
    if !wine::prefix_exists(&cfg) {
        return Err("Primero hay que crear el prefijo".into());
    }
    dxvk::install(&app, &cfg).await
}

#[tauri::command]
async fn install_battlenet(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let cfg = state.config();
    if !wine::prefix_exists(&cfg) {
        return Err("Primero hay que crear el prefijo".into());
    }
    wine::log(&app, "installer", "Descargando el instalador de Battle.net…");
    let setup = download::fetch(&app, "installer", INSTALLER_URL).await?;
    let cache = config::cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let setup_path = cache.join("Battle.net-Setup.exe");
    std::fs::write(&setup_path, setup).map_err(|e| e.to_string())?;

    wine::log(&app, "installer", "Ejecutando el instalador. Sigue los pasos en su ventana.");
    let mut cmd = wine::command(&cfg)?;
    cmd.arg(&setup_path)
        .arg("--lang=esES")
        .arg(format!("--installpath={}", wine::BATTLENET_DIR));
    wine::spawn_logged(&app, "installer", cmd);
    Ok(())
}

fn require_battlenet(cfg: &Config) -> Result<(), String> {
    if wine::battlenet_exe(cfg).is_file() {
        Ok(())
    } else {
        Err("Battle.net no está instalado en el prefijo".into())
    }
}

#[tauri::command]
fn launch_battlenet(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let cfg = state.config();
    require_battlenet(&cfg)?;
    if wine::battlenet_running(&wine::running_cmdlines()) {
        return Err("Battle.net ya está abierto".into());
    }
    wine::prepare_battlenet_config(&cfg)?;
    let mut cmd = wine::command(&cfg)?;
    cmd.arg(wine::battlenet_exe(&cfg));
    wine::spawn_logged(&app, "battlenet", cmd);
    Ok(())
}

#[tauri::command]
async fn close_battlenet(app: AppHandle) -> Result<(), String> {
    match wine::close_battlenet().await {
        0 => wine::log(&app, "battlenet", "Battle.net no estaba abierto"),
        n => wine::log(&app, "battlenet", format!("Battle.net cerrado ({n} procesos)")),
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
    let game = ea::find_game(&cfg, &id)?;
    ea::launch_game(&app, &cfg, &game).await
}

#[tauri::command]
async fn uninstall_ea_game(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let cfg = state.config().ea();
    let game = ea::games(&cfg, &wine::running_cmdlines())
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("Juego de EA desconocido: {id}"))?;
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
    xdg_open(&ea::find_game(&state.config().ea(), &id)?.path)
}

#[tauri::command]
fn open_ea_prefix(state: State<'_, AppState>) -> Result<(), String> {
    xdg_open(&state.config().ea().wine_prefix())
}

fn battlenet_games(cfg: &Config, cmdlines: &[String]) -> Result<Vec<Game>, String> {
    let Some(db) = productdb::read(&wine::product_db(cfg))? else {
        return Ok(Vec::new());
    };
    let mut games: Vec<Game> = db
        .product_installs
        .iter()
        .filter_map(|p| games::from_install(cfg, p, cmdlines))
        .collect();
    games.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(games)
}

fn battlenet_game(cfg: &Config, uid: &str) -> Result<Game, String> {
    battlenet_games(cfg, &wine::running_cmdlines())?
        .into_iter()
        .find(|g| g.uid == uid)
        .ok_or_else(|| format!("Juego desconocido: {uid}"))
}

#[tauri::command]
fn launch_game(app: AppHandle, state: State<'_, AppState>, uid: String) -> Result<(), String> {
    let cfg = state.config();
    require_battlenet(&cfg)?;
    let game = battlenet_game(&cfg, &uid)?;
    if !wine::battlenet_running(&wine::running_cmdlines()) {
        wine::prepare_battlenet_config(&cfg)?;
    }
    wine::log(&app, &uid, format!("Lanzando {} ({})", game.name, game.launch_code));
    let mut cmd = wine::command(&cfg)?;
    cmd.arg(wine::battlenet_exe(&cfg))
        .arg(format!("--exec=launch {}", game.launch_code));
    wine::spawn_logged(&app, &uid, cmd);
    Ok(())
}

#[tauri::command]
fn uninstall_game(app: AppHandle, state: State<'_, AppState>, uid: String) -> Result<(), String> {
    let cfg = state.config();
    games::uninstall(&app, &cfg, &battlenet_game(&cfg, &uid)?)
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

/// Data URL of the icon in a game's executable. `key` is the game's key in
/// the sidebar: a Battle.net uid, `ea:<id>` or `ext:<id>`.
#[tauri::command]
async fn game_icon(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    use base64::Engine;
    let cfg = state.config();
    tokio::task::spawn_blocking(move || {
        let exe = if let Some(id) = key.strip_prefix("ea:") {
            ea::find_game(&cfg.ea(), id).ok()?.exe?
        } else if let Some(id) = key.strip_prefix("ext:") {
            external::find(id).ok()?.exe?
        } else {
            // Battle.net doesn't say which executable is the game's.
            let game = battlenet_games(&cfg, &[]).ok()?.into_iter().find(|g| g.uid == key)?;
            disc::guess_exe(Path::new(&game.local_path))?
        };
        let icon = exeicon::read(&exe)?;
        Some(format!("data:{};base64,{}", icon.mime, base64::engine::general_purpose::STANDARD.encode(icon.data)))
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_game_folder(state: State<'_, AppState>, uid: String) -> Result<(), String> {
    xdg_open(Path::new(&battlenet_game(&state.config(), &uid)?.local_path))
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
            open_game_logs,
            list_protons,
            install_ge_proton,
            add_external_game,
            remove_external_game,
            launch_external_game,
            open_external_folder,
            set_external_env,
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
