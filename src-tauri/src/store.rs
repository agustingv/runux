//! The app's JSON files (`config.json`, `games.json`…), written so that a
//! crash or a full disk never leaves them half-written.

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The contents of `path`, or the default when it doesn't exist yet. A file
/// that can't be parsed is set aside as `<name>.corrupt` instead of being
/// overwritten by the next save, so its data can still be recovered.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    let Ok(text) = std::fs::read_to_string(path) else {
        return T::default();
    };
    match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            let backup = sibling(path, "corrupt");
            eprintln!("{} ilegible ({e}); se guarda como {}", path.display(), backup.display());
            let _ = std::fs::rename(path, &backup);
            T::default()
        }
    }
}

/// Writes `value` to a temporary file next to `path` and renames it over
/// `path`, which replaces the old contents all at once.
pub fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let tmp = sibling(path, "tmp");
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

/// `games.json` → `games.json.<ext>`.
fn sibling(path: &Path, ext: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(ext);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn saves_atomically_and_sets_corrupt_files_aside() {
        let dir = std::env::temp_dir().join(format!("runux-store-{}", std::process::id()));
        let path = dir.join("games.json");
        let _ = std::fs::remove_dir_all(&dir);

        assert!(load::<Vec<String>>(&path).is_empty());
        save(&path, &["a", "b"]).unwrap();
        assert_eq!(load::<Vec<String>>(&path), ["a", "b"]);
        assert!(!dir.join("games.json.tmp").exists());

        std::fs::write(&path, "[\"a\", ").unwrap();
        assert!(load::<BTreeMap<String, String>>(&path).is_empty());
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(dir.join("games.json.corrupt")).unwrap(), "[\"a\", ");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
