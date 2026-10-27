//! Discovers Proton builds installed through Steam or downloaded by the app,
//! and the Steam Linux Runtime container each one requires. Steam itself is
//! optional: GE-Proton runs without it.

use crate::{config, download, wine};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::AppHandle;

const GE_RELEASES: &str = "https://api.github.com/repos/GloriousEggroll/proton-ge-custom/releases/latest";
pub const GE_TASK: &str = "ge-proton";

/// A Steam Linux Runtime that Proton builds require by Steam app id, as Valve
/// publishes it outside Steam: a tarball holding `<dir>/_v2-entry-point`.
struct Runtime {
    appid: &'static str,
    name: &'static str,
    dir: &'static str,
    /// Folder with the tarball and its `SHA256SUMS`.
    base_url: &'static str,
}

const RUNTIMES: &[Runtime] = &[
    Runtime {
        appid: "4183110",
        name: "Steam Linux Runtime 4.0",
        dir: "SteamLinuxRuntime_4",
        base_url: "https://repo.steampowered.com/steamrt4/images/latest-public-stable",
    },
    Runtime {
        appid: "1628350",
        name: "Steam Linux Runtime 3.0 (sniper)",
        dir: "SteamLinuxRuntime_sniper",
        base_url: "https://repo.steampowered.com/steamrt3/images/latest-public-stable",
    },
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtonBuild {
    pub name: String,
    pub path: PathBuf,
    pub version: Option<String>,
    /// Steam Linux Runtime directory holding `_v2-entry-point`, when the build needs one.
    pub runtime: Option<PathBuf>,
    /// Name of the runtime the build needs but that isn't installed. Without
    /// it Proton runs on the host, which then needs Steam's 32-bit libraries.
    pub missing_runtime: Option<String>,
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
    /// Steam Linux Runtime downloaded along with it, if it was missing.
    pub runtime: Option<String>,
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

/// Where the app keeps the Steam Linux Runtimes it downloads.
fn runtimes_dir() -> PathBuf {
    config::data_dir().join("runtimes")
}

/// Steam app id of the runtime a Proton build runs in, from its manifest.
fn required_runtime(dir: &Path) -> Option<String> {
    let manifest = std::fs::read_to_string(dir.join("toolmanifest.vdf")).ok()?;
    vdf_values(&manifest, "require_tool_appid").into_iter().next()
}

/// A runtime the app downloaded, by Steam app id.
fn own_runtime(appid: &str) -> Option<PathBuf> {
    let rt = RUNTIMES.iter().find(|r| r.appid == appid)?;
    let dir = runtimes_dir().join(rt.dir);
    dir.join("_v2-entry-point").is_file().then_some(dir)
}

/// `STEAM_COMPAT_CLIENT_INSTALL_PATH` without Steam: Proton only copies
/// Steam's client DLLs from it when they exist.
fn fake_steam_root() -> PathBuf {
    config::data_dir().join("proton-client")
}

/// Removes the stand-in client dir's old name (`steam`) if it's still empty.
pub fn remove_old_client_dir() {
    let _ = std::fs::remove_dir(config::data_dir().join("steam"));
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
    let required = required_runtime(dir);
    let runtime = required.as_deref().and_then(|id| {
        find_app(libs, id)
            .filter(|rt| rt.join("_v2-entry-point").is_file())
            .or_else(|| own_runtime(id))
    });
    let missing_runtime = match (&required, &runtime) {
        (Some(id), None) => Some(RUNTIMES.iter().find(|r| r.appid == id).map_or_else(
            || format!("Steam Linux Runtime ({id})"),
            |r| r.name.to_string(),
        )),
        _ => None,
    };
    // `version` holds "<timestamp> <name>", e.g. "1788504981 proton-11.0-2c-x86_64".
    let version = std::fs::read_to_string(dir.join("version"))
        .ok()
        .and_then(|v| v.split_whitespace().nth(1).map(str::to_string));
    Some(ProtonBuild {
        name: dir.file_name()?.to_string_lossy().into_owned(),
        path: dir.to_path_buf(),
        version,
        runtime,
        missing_runtime,
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

/// How long a scan is reused: the UI asks for the runner every few seconds,
/// and scanning walks every Steam library.
const SCAN_TTL: Duration = Duration::from_secs(30);
static SCAN: Mutex<Option<(Instant, Vec<ProtonBuild>)>> = Mutex::new(None);

/// The Proton builds installed, best first; scanned at most every [`SCAN_TTL`].
pub fn discover() -> Vec<ProtonBuild> {
    let mut cache = SCAN.lock().unwrap_or_else(|e| e.into_inner());
    match &*cache {
        Some((at, builds)) if at.elapsed() < SCAN_TTL => builds.clone(),
        _ => {
            let builds = scan();
            *cache = Some((Instant::now(), builds.clone()));
            builds
        }
    }
}

/// Makes the next [`discover`] scan again, after builds were added.
pub fn forget_scan() {
    *SCAN.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn scan() -> Vec<ProtonBuild> {
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

/// Downloads the latest GE-Proton release into [`own_dir`], and the Steam
/// Linux Runtime it needs when neither Steam nor the app has it. Skips what is
/// already there; older releases are kept for whoever selected them.
pub async fn install_ge(app: &AppHandle) -> Result<GeInstall, String> {
    wine::log(app, GE_TASK, "Buscando la última versión de GE-Proton…");
    let release = download::latest_release(GE_RELEASES).await?;
    let asset_name = format!("{}-{}.tar.gz", release.tag_name, std::env::consts::ARCH);
    let find = |name: &str| release.assets.iter().find(|a| a.name == name);
    let asset = find(&asset_name)
        .ok_or_else(|| format!("La release de GE-Proton no tiene versión para {}", std::env::consts::ARCH))?;
    // The archive holds a single folder named like it, e.g. `GE-Proton11-11-x86_64/`.
    let name = asset_name.trim_end_matches(".tar.gz").to_string();
    let dest = own_dir().join(&name);

    let updated = !dest.join("proton").is_file();
    if updated {
        let sums_asset = find(&format!("{name}.sha512sum")).ok_or("La release de GE-Proton no trae su suma SHA-512")?;
        let sums = download::fetch_text(&sums_asset.browser_download_url).await?;
        let expected = checksum_for(&sums, &asset_name).ok_or("La suma SHA-512 de GE-Proton no nombra el archivo")?;
        let archive = download_verified::<sha2::Sha512>(app, &asset.browser_download_url, &asset_name, &expected).await?;
        wine::log(app, GE_TASK, "Descomprimiendo GE-Proton…");
        let dir = own_dir();
        let entry = name.clone();
        unpack(archive, dir, entry, |p| p.join("proton").is_file(), false).await?;
        wine::log(app, GE_TASK, format!("{} instalado en {}", release.tag_name, dest.display()));
    } else {
        wine::log(app, GE_TASK, format!("{} ya está instalado", release.tag_name));
    }

    let runtime = match build_from_dir(&dest, &steam_libs(), &fake_steam_root()) {
        Some(build) if build.missing_runtime.is_some() => {
            let id = required_runtime(&dest).unwrap_or_default();
            Some(install_runtime(app, &id).await?)
        }
        _ => None,
    };
    Ok(GeInstall { version: release.tag_name, updated, runtime })
}

/// Libraries of the first Steam install, to find runtimes Steam already has.
fn steam_libs() -> Vec<PathBuf> {
    default_root(&steam_roots()).0
}

/// Downloads and unpacks a Steam Linux Runtime from Valve's servers.
async fn install_runtime(app: &AppHandle, appid: &str) -> Result<String, String> {
    let rt = RUNTIMES
        .iter()
        .find(|r| r.appid == appid)
        .ok_or_else(|| format!("GE-Proton necesita un Steam Linux Runtime desconocido ({appid})"))?;
    let file = format!("{}.tar.xz", rt.dir);
    wine::log(app, GE_TASK, format!("GE-Proton se ejecuta dentro de {}. Descargándolo…", rt.name));
    let sums = download::fetch_text(&format!("{}/SHA256SUMS", rt.base_url)).await?;
    let expected = checksum_for(&sums, &file).ok_or_else(|| format!("SHA256SUMS no incluye {file}"))?;
    let archive = download_verified::<sha2::Sha256>(app, &format!("{}/{file}", rt.base_url), &file, &expected).await?;
    wine::log(app, GE_TASK, format!("Descomprimiendo {}…", rt.name));
    unpack(archive, runtimes_dir(), rt.dir.to_string(), |p| p.join("_v2-entry-point").is_file(), true).await?;
    wine::log(app, GE_TASK, format!("{} instalado", rt.name));
    Ok(rt.name.to_string())
}

/// The hash for `file` in a `sha256sum`-style list (`<hash>  <file>` or `<hash> *<file>`).
fn checksum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then(|| hash.to_ascii_lowercase())
    })
}

/// Downloads `url` into the cache as `file` and checks it against `expected`
/// (hex), deleting it if it doesn't match.
async fn download_verified<D: sha2::Digest>(app: &AppHandle, url: &str, file: &str, expected: &str) -> Result<PathBuf, String> {
    wine::log(app, GE_TASK, format!("Descargando {file}"));
    let archive = download::fetch_to_cache(app, GE_TASK, url, file).await?;
    let path = archive.clone();
    let actual = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut hasher = D::new();
        let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut DigestWriter(&mut hasher)).map_err(|e| e.to_string())?;
        Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
    })
    .await
    .map_err(|e| e.to_string())??;
    if actual != expected {
        let _ = std::fs::remove_file(&archive);
        return Err(format!("{file} se ha descargado mal (la suma de comprobación no coincide). Vuelve a intentarlo."));
    }
    Ok(archive)
}

struct DigestWriter<'a, D>(&'a mut D);

impl<D: sha2::Digest> std::io::Write for DigestWriter<'_, D> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Unpacks the `.tar.gz` (or `.tar.xz` when `xz`) `archive`, which holds a
/// single folder `entry`, into `dir/entry`, then deletes the archive. Goes
/// through a temporary dir so an interrupted extraction never looks usable;
/// `valid` checks the unpacked folder.
async fn unpack(archive: PathBuf, dir: PathBuf, entry: String, valid: fn(&Path) -> bool, xz: bool) -> Result<(), String> {
    let result = tauri::async_runtime::spawn_blocking({
        let archive = archive.clone();
        move || -> Result<(), String> {
            let tmp = dir.join(format!(".{entry}.partial"));
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
            let file = std::fs::File::open(&archive).map_err(|e| e.to_string())?;
            let reader: Box<dyn std::io::Read> = if xz {
                Box::new(xz2::read::XzDecoder::new(file))
            } else {
                Box::new(flate2::read::GzDecoder::new(file))
            };
            let mut tar = tar::Archive::new(reader);
            tar.set_preserve_permissions(true);
            let unpacked = tar
                .unpack(&tmp)
                .map_err(|e| format!("Error descomprimiendo {}: {e}", archive.display()))
                .map(|()| tmp.join(&entry));
            let unpacked = match unpacked {
                Ok(p) if valid(&p) => p,
                Ok(_) => {
                    let _ = std::fs::remove_dir_all(&tmp);
                    return Err(format!("El archivo no contiene una carpeta {entry} válida"));
                }
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&tmp);
                    return Err(e);
                }
            };
            let dest = dir.join(&entry);
            let _ = std::fs::remove_dir_all(&dest);
            std::fs::rename(&unpacked, &dest).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_dir_all(&tmp);
            Ok(())
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&archive);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unpacks_an_xz_archive_through_a_temporary_dir() {
        let base = std::env::temp_dir().join(format!("runux-unpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src/SteamLinuxRuntime_4");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("_v2-entry-point"), "#!/bin/sh\n").unwrap();
        let archive = base.join("rt.tar.xz");
        {
            let mut tar = tar::Builder::new(xz2::write::XzEncoder::new(std::fs::File::create(&archive).unwrap(), 6));
            tar.append_dir_all("SteamLinuxRuntime_4", &src).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let dir = base.join("runtimes");
        unpack(archive.clone(), dir.clone(), "SteamLinuxRuntime_4".into(), |p| p.join("_v2-entry-point").is_file(), true)
            .await
            .unwrap();
        assert!(dir.join("SteamLinuxRuntime_4/_v2-entry-point").is_file());
        assert!(!dir.join(".SteamLinuxRuntime_4.partial").exists());
        assert!(!archive.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn finds_checksums_in_both_sum_formats() {
        let sums = "ABC123 *SteamLinuxRuntime_4.tar.xz\nfff  other.tar.xz\n";
        assert_eq!(checksum_for(sums, "SteamLinuxRuntime_4.tar.xz").as_deref(), Some("abc123"));
        assert_eq!(checksum_for("deadbeef  GE-Proton11-11-x86_64.tar.gz\n", "GE-Proton11-11-x86_64.tar.gz").as_deref(), Some("deadbeef"));
        assert_eq!(checksum_for(sums, "missing.tar.xz"), None);
    }

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
            missing_runtime: None,
            steam_root: "/".into(),
            downloaded: false,
        };
        let mut builds = vec![b("Proton 9.0 (Beta)"), b("Proton Hotfix"), b("Proton 11.0"), b("Proton - Experimental"), b("GE-Proton10-25"), b("Proton 10.0")];
        builds.sort_by_key(rank);
        let names: Vec<_> = builds.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["GE-Proton10-25", "Proton - Experimental", "Proton 11.0", "Proton 10.0", "Proton 9.0 (Beta)", "Proton Hotfix"]);
    }
}
