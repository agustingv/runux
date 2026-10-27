use crate::config::Config;
use crate::productdb::ProductInstall;
use crate::wine;
use serde::Serialize;
use tauri::AppHandle;

struct KnownGame {
    /// `uid` or `product_code` values found in product.db.
    ids: &'static [&'static str],
    name: &'static str,
    /// Code accepted by `Battle.net.exe --exec="launch <code>"`.
    launch: &'static str,
    accent: &'static str,
}

const CATALOG: &[KnownGame] = &[
    KnownGame { ids: &["wow"], name: "World of Warcraft", launch: "WoW", accent: "#f8b700" },
    KnownGame { ids: &["wow_classic"], name: "WoW Classic", launch: "WoWC", accent: "#c79c6e" },
    KnownGame { ids: &["wow_classic_era"], name: "WoW Classic Era", launch: "WoWCE", accent: "#a67c52" },
    KnownGame { ids: &["fenris", "fen"], name: "Diablo IV", launch: "Fen", accent: "#b3261e" },
    KnownGame { ids: &["osi"], name: "Diablo II: Resurrected", launch: "OSI", accent: "#8e1b1b" },
    KnownGame { ids: &["diablo3", "d3"], name: "Diablo III", launch: "D3", accent: "#c0392b" },
    KnownGame { ids: &["anbs"], name: "Diablo Immortal", launch: "ANBS", accent: "#a93226" },
    KnownGame { ids: &["prometheus", "pro"], name: "Overwatch 2", launch: "Pro", accent: "#f99e1a" },
    KnownGame { ids: &["hs_beta", "wtcg"], name: "Hearthstone", launch: "WTCG", accent: "#e0a526" },
    KnownGame { ids: &["heroes", "hero"], name: "Heroes of the Storm", launch: "Hero", accent: "#7b4fd6" },
    KnownGame { ids: &["s2"], name: "StarCraft II", launch: "S2", accent: "#2f8fdc" },
    KnownGame { ids: &["s1"], name: "StarCraft Remastered", launch: "S1", accent: "#3d6fb6" },
    KnownGame { ids: &["w3"], name: "Warcraft III: Reforged", launch: "W3", accent: "#2e86c1" },
    KnownGame { ids: &["gryphon", "gry"], name: "Warcraft Rumble", launch: "GRY", accent: "#e67e22" },
    KnownGame { ids: &["rtro"], name: "Blizzard Arcade Collection", launch: "RTRO", accent: "#16a085" },
    KnownGame { ids: &["wlby"], name: "Crash Bandicoot 4", launch: "WLBY", accent: "#e67e22" },
    KnownGame { ids: &["auks"], name: "Call of Duty", launch: "AUKS", accent: "#6b8e23" },
    KnownGame { ids: &["odin"], name: "Call of Duty: Modern Warfare", launch: "ODIN", accent: "#556b2f" },
    KnownGame { ids: &["lazarus", "lazr"], name: "Call of Duty: MWII", launch: "LAZR", accent: "#4f6d3a" },
    KnownGame { ids: &["fore"], name: "Call of Duty: Vanguard", launch: "FORE", accent: "#8b6b3d" },
    KnownGame { ids: &["zeus"], name: "Call of Duty: Black Ops Cold War", launch: "ZEUS", accent: "#7d3c98" },
    KnownGame { ids: &["viper", "vipr"], name: "Call of Duty: Black Ops 4", launch: "VIPR", accent: "#d35400" },
];

/// Entries in product.db that are Battle.net components, not games.
const IGNORED: &[&str] = &["agent", "battle.net", "bna", "bts"];

fn lookup(uid: &str, code: &str) -> Option<&'static KnownGame> {
    let uid = uid.to_lowercase();
    let code = code.to_lowercase();
    CATALOG
        .iter()
        .find(|g| g.ids.contains(&uid.as_str()))
        .or_else(|| CATALOG.iter().find(|g| g.ids.contains(&code.as_str())))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub uid: String,
    pub product_code: String,
    pub name: String,
    pub launch_code: String,
    pub accent: String,
    pub install_path: String,
    pub local_path: String,
    pub version: String,
    pub region: String,
    pub installed: bool,
    pub playable: bool,
    pub update_complete: bool,
    /// 0.0–1.0 while Battle.net is downloading an update.
    pub update_progress: Option<f64>,
    pub running: bool,
}

pub fn from_install(cfg: &Config, p: &ProductInstall, cmdlines: &[String]) -> Option<Game> {
    let uid_lc = p.uid.to_lowercase();
    if IGNORED.contains(&uid_lc.as_str()) || IGNORED.contains(&p.product_code.to_lowercase().as_str()) {
        return None;
    }
    let known = lookup(&p.uid, &p.product_code);
    let settings = p.settings.clone().unwrap_or_default();
    let state = p.cached_product_state.clone().unwrap_or_default();
    let base = state.base_product_state.unwrap_or_default();
    let update_progress = state
        .update_progress
        .filter(|u| !base.update_complete && u.progress > 0.0 && u.progress < 1.0)
        .map(|u| u.progress);

    let install_path = settings.install_path;
    // With the separator, so `…/World of Warcraft` doesn't match `…/World of Warcraft Classic`.
    let norm_path = wine::normalize_win_path(&install_path);
    let inside = format!("{norm_path}/");
    let running = !norm_path.is_empty() && cmdlines.iter().any(|c| c.contains(&inside));

    Some(Game {
        name: known.map_or_else(|| p.product_code.clone(), |k| k.name.into()),
        launch_code: known.map_or_else(|| p.product_code.clone(), |k| k.launch.into()),
        accent: known.map_or("#148eff", |k| k.accent).into(),
        local_path: wine::to_unix_path(cfg, &install_path).to_string_lossy().into(),
        install_path,
        version: base.current_version_str,
        region: settings.play_region,
        installed: base.installed,
        playable: base.playable,
        update_complete: base.update_complete,
        update_progress,
        running,
        uid: p.uid.clone(),
        product_code: p.product_code.clone(),
    })
}

/// Blizzard's own uninstaller, shared by every game; it's what the game's
/// entry in Windows' "Apps & features" runs.
const UNINSTALLER: &str = r"C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe";

/// Task of a game's uninstaller.
pub fn uninstall_task(uid: &str) -> String {
    format!("uninstall:{uid}")
}

pub fn uninstall(app: &AppHandle, cfg: &Config, game: &Game) -> Result<(), String> {
    if game.running {
        return Err(format!("Cierra {} antes de desinstalarlo", game.name));
    }
    let exe = wine::to_unix_path(cfg, UNINSTALLER);
    if !exe.is_file() {
        return Err("No se encuentra el desinstalador de Blizzard en el prefijo".into());
    }
    let task = uninstall_task(&game.uid);
    wine::log(app, &task, format!("Desinstalando {}", game.name));
    // The plain wine binary, so the task ends when the uninstaller does.
    let mut cmd = wine::tool_command(cfg)?;
    cmd.arg(exe)
        .arg("--lang=esES")
        .arg(format!("--uid={}", game.uid))
        .arg(format!("--displayname={}", game.name))
        .current_dir(cfg.wine_prefix());
    wine::spawn_logged(app, &task, cmd);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::productdb::{self, BaseProductState, CachedProductState, Database, UserSettings};
    use prost::Message;

    fn install(uid: &str, code: &str, path: &str) -> ProductInstall {
        ProductInstall {
            uid: uid.into(),
            product_code: code.into(),
            settings: Some(UserSettings {
                install_path: path.into(),
                play_region: "eu".into(),
                ..Default::default()
            }),
            cached_product_state: Some(CachedProductState {
                base_product_state: Some(BaseProductState {
                    installed: true,
                    playable: true,
                    update_complete: true,
                    current_version_str: "11.0.5.57212".into(),
                }),
                update_progress: None,
            }),
        }
    }

    #[test]
    fn reads_product_db_and_maps_games() {
        let cfg = Config { runner: crate::config::Runner::Wine, prefix: "/pfx".into(), ..Default::default() };
        let db = Database {
            product_installs: vec![
                install("agent", "agent", ""),
                install("wow", "wow", "C:/Program Files (x86)/World of Warcraft"),
                install("wow_classic", "wow_classic", "C:/Program Files (x86)/World of Warcraft"),
                install("prometheus", "pro", "C:/Program Files (x86)/Overwatch"),
                install("mystery", "mys", "C:/Games/Mystery"),
            ],
        };
        let dir = std::env::temp_dir().join(format!("runux-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("product.db");
        std::fs::write(&path, db.encode_to_vec()).unwrap();

        let parsed = productdb::read(&path).unwrap().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let cmdlines = vec![
            "c:/program files (x86)/overwatch/overwatch.exe ".to_string(),
            "c:/games/mystery2/mystery.exe ".to_string(),
        ];
        let games: Vec<Game> = parsed
            .product_installs
            .iter()
            .filter_map(|p| from_install(&cfg, p, &cmdlines))
            .collect();

        let names: Vec<_> = games.iter().map(|g| (g.name.as_str(), g.launch_code.as_str())).collect();
        assert_eq!(
            names,
            [("World of Warcraft", "WoW"), ("WoW Classic", "WoWC"), ("Overwatch 2", "Pro"), ("mys", "mys")]
        );
        assert!(games[2].running && !games[0].running && !games[3].running);
        assert_eq!(games[0].version, "11.0.5.57212");
        assert_eq!(
            games[0].local_path,
            "/pfx/dosdevices/c:/Program Files (x86)/World of Warcraft"
        );
    }

    #[test]
    fn missing_product_db_is_not_an_error() {
        assert!(productdb::read(std::path::Path::new("/nonexistent/product.db")).unwrap().is_none());
    }
}

