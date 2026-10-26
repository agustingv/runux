use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runner {
    /// Valve's Proton, run inside its Steam Linux Runtime container.
    #[default]
    Proton,
    /// A plain wine installation.
    Wine,
}

/// Persistent launcher settings, stored as JSON in `~/.config/runux/config.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub runner: Runner,
    /// Wine binary to use: either a name on `$PATH` or an absolute path.
    pub wine_bin: String,
    /// Wine prefix used by the `wine` runner.
    pub prefix: PathBuf,
    /// Proton build directory; `None` picks the best one installed.
    pub proton_path: Option<PathBuf>,
    /// `STEAM_COMPAT_DATA_PATH` for the `proton` runner (the prefix lives in `pfx/`).
    pub proton_data: PathBuf,
    /// Wine prefix of the EA app, kept apart from Battle.net's.
    pub ea_prefix: PathBuf,
    /// `STEAM_COMPAT_DATA_PATH` of the EA app with the `proton` runner.
    pub ea_proton_data: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            runner: Runner::default(),
            wine_bin: "wine".into(),
            prefix: data_dir().join("prefix"),
            proton_path: None,
            proton_data: data_dir().join("proton"),
            ea_prefix: data_dir().join("ea").join("prefix"),
            ea_proton_data: data_dir().join("ea").join("proton"),
        }
    }
}

impl Config {
    fn path() -> PathBuf {
        config_dir().join("config.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// The same settings pointed at the EA app's prefix, so everything that
    /// runs in a prefix works for it unchanged.
    pub fn ea(&self) -> Config {
        Config {
            prefix: self.ea_prefix.clone(),
            proton_data: self.ea_proton_data.clone(),
            ..self.clone()
        }
    }

    /// The actual `WINEPREFIX`, whichever runner is active.
    pub fn wine_prefix(&self) -> PathBuf {
        match self.runner {
            Runner::Wine => self.prefix.clone(),
            Runner::Proton => self.proton_data.join("pfx"),
        }
    }
}

const APP_DIR: &str = "runux";
/// The app's dir name before it was renamed from BTN Launcher to Runux.
const OLD_APP_DIR: &str = "btn";

fn base(dir: Option<PathBuf>) -> PathBuf {
    dir.unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_dir() -> PathBuf {
    base(dirs::config_dir()).join(APP_DIR)
}

pub fn data_dir() -> PathBuf {
    base(dirs::data_dir()).join(APP_DIR)
}

pub fn cache_dir() -> PathBuf {
    base(dirs::cache_dir()).join(APP_DIR)
}

/// Moves the dirs of BTN Launcher to Runux's, leaving a symlink behind so
/// anything that still holds an old path (a running wineserver, a shortcut)
/// keeps working, and repoints the paths stored in the settings files.
pub fn migrate_from_btn() {
    let mut moved = false;
    for parent in [dirs::config_dir(), dirs::data_dir(), dirs::cache_dir()].into_iter().flatten() {
        let (old, new) = (parent.join(OLD_APP_DIR), parent.join(APP_DIR));
        let is_real_dir = old.symlink_metadata().is_ok_and(|m| m.is_dir());
        if !is_real_dir || new.exists() {
            continue;
        }
        if let Err(e) = std::fs::rename(&old, &new) {
            eprintln!("No se pudo mover {} a {}: {e}", old.display(), new.display());
            continue;
        }
        let _ = std::os::unix::fs::symlink(APP_DIR, &old);
        moved = true;
    }
    if !moved {
        return;
    }
    let old_data = base(dirs::data_dir()).join(OLD_APP_DIR);
    let Ok(entries) = std::fs::read_dir(config_dir()) else { return };
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")) {
        let Some(mut json) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        else {
            continue;
        };
        if relocate(&mut json, &old_data, &data_dir()) {
            if let Ok(text) = serde_json::to_string_pretty(&json) {
                let _ = std::fs::write(&path, text);
            }
        }
    }
}

/// Rewrites every string in `value` that is a path under `old` to the same
/// path under `new`; returns whether anything changed.
fn relocate(value: &mut serde_json::Value, old: &Path, new: &Path) -> bool {
    use serde_json::Value;
    match value {
        Value::String(s) => match Path::new(s.as_str()).strip_prefix(old) {
            Ok(rest) => {
                let moved = if rest.as_os_str().is_empty() { new.to_path_buf() } else { new.join(rest) };
                *s = moved.to_string_lossy().into_owned();
                true
            }
            Err(_) => false,
        },
        Value::Array(items) => items.iter_mut().fold(false, |changed, v| relocate(v, old, new) | changed),
        Value::Object(map) => map.values_mut().fold(false, |changed, v| relocate(v, old, new) | changed),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocates_paths_under_the_old_dir_only() {
        let mut json = serde_json::json!({
            "prefix": "/home/u/.local/share/btn/prefix",
            "root": "/home/u/.local/share/btn",
            "games": [{ "path": "/home/u/.local/share/btn/games/x", "exe": "/mnt/btn/x.exe" }],
            "other": "/home/u/.local/share/btnx",
            "n": 3,
        });
        assert!(relocate(&mut json, Path::new("/home/u/.local/share/btn"), Path::new("/home/u/.local/share/runux")));
        assert_eq!(json, serde_json::json!({
            "prefix": "/home/u/.local/share/runux/prefix",
            "root": "/home/u/.local/share/runux",
            "games": [{ "path": "/home/u/.local/share/runux/games/x", "exe": "/mnt/btn/x.exe" }],
            "other": "/home/u/.local/share/btnx",
            "n": 3,
        }));
    }
}
