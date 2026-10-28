//! The app's JSON files (`config.json`, `games.json`…), written so that a
//! crash or a full disk never leaves them half-written.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The contents of `path`, or the default when it doesn't exist yet. A file
/// that can't be parsed is set aside as `<name>.corrupt-<time>` instead of
/// being overwritten by the next save, so its data can still be recovered.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    let Ok(text) = std::fs::read_to_string(path) else {
        return T::default();
    };
    match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            let backup = corrupt_path(path);
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

/// A JSON list at `path`: the entries that read as `T`, and the others, raw.
/// Those come from a newer version (a kind of game this one doesn't know):
/// they're left out instead of making the whole file unreadable, and
/// `save_list` writes them back.
pub fn load_list<T: DeserializeOwned>(path: &Path) -> (Vec<T>, Vec<Value>) {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for value in load::<Vec<Value>>(path) {
        match T::deserialize(&value) {
            Ok(item) => known.push(item),
            Err(_) => unknown.push(value),
        }
    }
    (known, unknown)
}

/// Saves `items` as the list at `path`, keeping the entries already there
/// that this version doesn't understand.
pub fn save_list<T: Serialize + DeserializeOwned>(path: &Path, items: &[T]) -> Result<(), String> {
    let (_, unknown) = load_list::<T>(path);
    let mut all: Vec<Value> = items.iter().map(serde_json::to_value).collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    all.extend(unknown);
    save(path, &all)
}

/// `games.json` → `games.json.corrupt-<seconds>`, a new name each time, so
/// a copy set aside earlier is never overwritten.
fn corrupt_path(path: &Path) -> PathBuf {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    (0..)
        .map(|n| sibling(path, &if n == 0 { format!("corrupt-{secs}") } else { format!("corrupt-{secs}-{n}") }))
        .find(|p| !p.exists())
        .expect("a free name")
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
    use serde::Deserialize;
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

        // Set aside twice: the first copy survives the second.
        let corrupt = |text: &str| {
            std::fs::write(&path, text).unwrap();
            assert!(load::<BTreeMap<String, String>>(&path).is_empty());
            assert!(!path.exists());
        };
        corrupt("[\"a\", ");
        corrupt("[\"b\", ");
        let mut copies: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
            .collect();
        copies.sort();
        assert_eq!(copies, ["[\"a\", ", "[\"b\", "]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keeps_entries_it_does_not_understand() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        #[serde(rename_all = "lowercase")]
        enum Kind {
            Windows,
        }
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Game {
            id: String,
            kind: Kind,
        }
        let dir = std::env::temp_dir().join(format!("runux-list-{}", std::process::id()));
        let path = dir.join("games.json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"[{"id":"re4","kind":"windows"},{"id":"doa2","kind":"dreamcast"}]"#).unwrap();

        let (games, unknown) = load_list::<Game>(&path);
        assert_eq!(games, [Game { id: "re4".into(), kind: Kind::Windows }]);
        assert_eq!(unknown.len(), 1);

        save_list(&path, &[Game { id: "alive".into(), kind: Kind::Windows }]).unwrap();
        let saved: Vec<Value> = load(&path);
        let ids: Vec<_> = saved.iter().map(|g| g["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["alive", "doa2"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
