//! Discovers Proton builds installed through Steam or downloaded by the app,
//! and the Steam Linux Runtime container each one requires. Steam itself is
//! optional: GE-Proton runs without it.

use crate::{config, download, wine};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const GE_RELEASES: &str = "https://api.github.com/repos/GloriousEggroll/proton-ge-custom/releases/latest";
pub const GE_TASK: &str = "ge-proton";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtonBuild {
    pub name: String,
    pub path: PathBuf,
    pub version: Option<String>,
    /// Steam Linux Runtime directory holding `_v2-entry-point`, when the build needs one.
    pub runtime: Option<PathBuf>,
    pub steam_root: PathBuf,
    /// Downloaded by the app (see [`install_ge`]) rather than found in Steam.
    pub downloaded: bool,
}

/// Outcome of [`install_ge`].
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeInstall {
    /// Release tag, e.g. `GE-Proton11-11`.
    pub version: String,
    /// False when that release was already installed.
    pub updated: bool,
}

/// Values of every `"key" "value"` line in a VDF/ACF file. Good enough for the flat
/// lookups needed here; nesting is ignored.
fn vdf_values(text: &str, key: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let tokens: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
            (tokens.len() >= 2 && tokens[0].eq_ignore_ascii_case(key)).then(|| tokens[1].replace("\\\\", "\\"))
        })
        .collect()
}

fn steam_roots() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = [".steam/root", ".steam/steam", ".local/share/Steam"]
        .iter()
        .filter_map(|p| std::fs::canonicalize(home.join(p)).ok())
        .filter(|p| p.join("steamapps").is_dir())
        .collect();
    roots.dedup();
    roots
}

fn library_folders(root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![root.to_path_buf()];
    if let Ok(text) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        for path in vdf_values(&text, "path") {
            let path = std::fs::canonicalize(&path).unwrap_or_else(|_| PathBuf::from(path));
            if path.join("steamapps").is_dir() && !libs.contains(&path) {
                libs.push(path);
            }
        }
    }
    libs
}

/// Where the app keeps the Proton builds it downloads.
pub fn own_dir() -> PathBuf {
    config::data_dir().join("compatibilitytools")
}

/// `STEAM_COMPAT_CLIENT_INSTALL_PATH` without Steam: Proton only copies
/// Steam's client DLLs from it when they exist.
fn fake_steam_root() -> PathBuf {
    config::data_dir().join("steam")
}

/// Finds the install dir of a Steam app (e.g. the runtime a Proton build requires).
fn find_app(libs: &[PathBuf], appid: &str) -> Option<PathBuf> {
    libs.iter().find_map(|lib| {
        let acf = std::fs::read_to_string(lib.join(format!("steamapps/appmanifest_{appid}.acf"))).ok()?;
        let dir = lib.join("steamapps/common").join(vdf_values(&acf, "installdir").first()?);
        dir.is_dir().then_some(dir)
    })
}

fn build_from_dir(dir: &Path, libs: &[PathBuf], steam_root: &Path) -> Option<ProtonBuild> {
    if !dir.join("proton").is_file() {
        return None;
    }
    let manifest = std::fs::read_to_string(dir.join("toolmanifest.vdf")).unwrap_or_default();
    let runtime = vdf_values(&manifest, "require_tool_appid")
        .first()
        .and_then(|id| find_app(libs, id))
        .filter(|rt| rt.join("_v2-entry-point").is_file());
    // `version` holds "<timestamp> <name>", e.g. "1788504981 proton-11.0-2c-x86_64".
    let version = std::fs::read_to_string(dir.join("version"))
        .ok()
        .and_then(|v| v.split_whitespace().nth(1).map(str::to_string));
    Some(ProtonBuild {
        name: dir.file_name()?.to_string_lossy().into_owned(),
        path: dir.to_path_buf(),
        version,
        runtime,
        steam_root: steam_root.to_path_buf(),
        downloaded: dir.starts_with(own_dir()),
    })
}

/// Lower sorts first: GE-Proton, Experimental, then numbered releases (newest first).
fn rank(build: &ProtonBuild) -> (u8, i64) {
    let name = build.name.to_lowercase();
    let number: f64 = name
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find_map(|s| s.parse().ok())
        .unwrap_or(0.0);
    let class = if name.contains("ge-proton") {
        0
    } else if name.contains("experimental") {
        1
    } else {
        2
    };
    (class, -(number * 100.0) as i64)
}

pub fn discover() -> Vec<ProtonBuild> {
    let mut builds = Vec::new();
    let mut add = |dir: &Path, libs: &[PathBuf], root: &Path| {
        if let Some(b) = build_from_dir(dir, libs, root) {
            if !builds.iter().any(|x: &ProtonBuild| x.path == b.path) {
                builds.push(b);
            }
        }
    };
    let roots = steam_roots();
    for root in &roots {
        let libs = library_folders(root);
        let mut dirs: Vec<PathBuf> = Vec::new();
        for lib in &libs {
            if let Ok(entries) = std::fs::read_dir(lib.join("steamapps/common")) {
                dirs.extend(entries.flatten().map(|e| e.path()));
            }
        }
        if let Ok(entries) = std::fs::read_dir(root.join("compatibilitytools.d")) {
            dirs.extend(entries.flatten().map(|e| e.path()));
        }
        for dir in dirs {
            add(&dir, &libs, root);
        }
    }
    if let Ok(entries) = std::fs::read_dir(own_dir()) {
        let (libs, root) = default_root(&roots);
        for entry in entries.flatten() {
            add(&entry.path(), &libs, &root);
        }
    }
    builds.sort_by_key(rank);
    builds
}

/// Steam's libraries (to find the runtime) and client dir, or none and an
/// empty stand-in dir when Steam isn't installed.
fn default_root(roots: &[PathBuf]) -> (Vec<PathBuf>, PathBuf) {
    match roots.first() {
        Some(root) => (library_folders(root), root.clone()),
        None => (Vec::new(), fake_steam_root()),
    }
}

/// The configured build, or the best one found when none is set.
pub fn resolve(selected: Option<&Path>) -> Option<ProtonBuild> {
    let builds = discover();
    match selected {
        Some(path) => builds.into_iter().find(|b| b.path == path).or_else(|| {
            // A build outside the known folders (e.g. a manually unpacked GE-Proton).
            let (libs, root) = default_root(&steam_roots());
            build_from_dir(path, &libs, &root)
        }),
        None => builds.into_iter().next(),
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Downloads the latest GE-Proton release into [`own_dir`]. Does nothing if
/// that release is already there; older ones are kept for whoever selected them.
pub async fn install_ge(app: &AppHandle) -> Result<GeInstall, String> {
    wine::log(app, GE_TASK, "Buscando la última versión de GE-Proton…");
    let release: Release = download::client()?
        .get(GE_RELEASES)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let asset_name = format!("{}-{}.tar.gz", release.tag_name, std::env::consts::ARCH);
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| format!("La release de GE-Proton no tiene versión para {}", std::env::consts::ARCH))?;
    // The archive holds a single folder named like it, e.g. `GE-Proton11-11-x86_64/`.
    let name = asset_name.trim_end_matches(".tar.gz").to_string();
    let dest = own_dir().join(&name);
    if dest.join("proton").is_file() {
        wine::log(app, GE_TASK, format!("{} ya está instalado", release.tag_name));
        return Ok(GeInstall { version: release.tag_name, updated: false });
    }

    let cache = config::cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let archive = cache.join(&asset.name);
    wine::log(app, GE_TASK, format!("Descargando {}", asset.name));
    download::fetch_to_file(app, GE_TASK, &asset.browser_download_url, &archive).await?;

    wine::log(app, GE_TASK, "Descomprimiendo…");
    let result = tauri::async_runtime::spawn_blocking({
        let archive = archive.clone();
        move || extract(&archive, &name)
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&archive);
    result?;
    wine::log(app, GE_TASK, format!("{} instalado en {}", release.tag_name, dest.display()));
    Ok(GeInstall { version: release.tag_name, updated: true })
}

/// Unpacks `<name>/…` from the archive into [`own_dir`], through a temporary
/// dir so an interrupted extraction never looks like a usable build.
fn extract(archive: &Path, name: &str) -> Result<(), String> {
    let dir = own_dir();
    let tmp = dir.join(format!(".{name}.partial"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    tar::Archive::new(flate2::read::GzDecoder::new(file))
        .unpack(&tmp)
        .map_err(|e| format!("Error descomprimiendo GE-Proton: {e}"))?;
    let unpacked = tmp.join(name);
    if !unpacked.join("proton").is_file() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("El archivo no contiene {name}/proton"));
    }
    let dest = dir.join(name);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&unpacked, &dest).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_vdf_values() {
        let text = "\"manifest\"\n{\n  \"require_tool_appid\" \"4183110\"\n\t\t\"path\"\t\t\"/mnt/games\"\n}";
        assert_eq!(vdf_values(text, "require_tool_appid"), ["4183110"]);
        assert_eq!(vdf_values(text, "path"), ["/mnt/games"]);
        assert!(vdf_values(text, "missing").is_empty());
    }

    #[test]
    fn ranks_experimental_before_numbered_and_newest_first() {
        let b = |name: &str| ProtonBuild {
            name: name.into(),
            path: name.into(),
            version: None,
            runtime: None,
            steam_root: "/".into(),
            downloaded: false,
        };
        let mut builds = vec![b("Proton 9.0 (Beta)"), b("Proton Hotfix"), b("Proton 11.0"), b("Proton - Experimental"), b("GE-Proton10-25"), b("Proton 10.0")];
        builds.sort_by_key(rank);
        let names: Vec<_> = builds.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["GE-Proton10-25", "Proton - Experimental", "Proton 11.0", "Proton 10.0", "Proton 9.0 (Beta)", "Proton Hotfix"]);
    }
}
