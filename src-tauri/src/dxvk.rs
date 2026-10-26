//! Installs the latest DXVK release into the prefix (Direct3D 8–11 over Vulkan).

use crate::config::Config;
use crate::{download, wine};
use serde::Deserialize;
use std::path::{Component, Path};
use tauri::AppHandle;

const RELEASES: &str = "https://api.github.com/repos/doitsujin/dxvk/releases/latest";
const DLLS: &[&str] = &["d3d8", "d3d9", "d3d10core", "d3d11", "dxgi"];
const MARKER: &str = ".btn-dxvk";

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

pub fn installed_version(cfg: &Config) -> Option<String> {
    std::fs::read_to_string(cfg.wine_prefix().join(MARKER))
        .ok()
        .map(|s| s.trim().to_string())
}

pub async fn install(app: &AppHandle, cfg: &Config) -> Result<String, String> {
    const TASK: &str = "dxvk";
    wine::log(app, TASK, "Buscando la última versión de DXVK…");
    let release: Release = download::client()?
        .get(RELEASES)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.starts_with("dxvk-") && a.name.ends_with(".tar.gz") && !a.name.contains("native"))
        .ok_or("La release de DXVK no contiene un .tar.gz")?;

    wine::log(app, TASK, format!("Descargando {}", asset.name));
    let archive = download::fetch(app, TASK, &asset.browser_download_url).await?;

    let prefix = cfg.wine_prefix();
    let copied = tauri::async_runtime::spawn_blocking(move || extract(&archive, &prefix))
        .await
        .map_err(|e| e.to_string())??;
    wine::log(app, TASK, format!("{copied} DLLs copiadas"));

    for dll in DLLS {
        let mut cmd = wine::command(cfg)?;
        cmd.args(["reg", "add", r"HKCU\Software\Wine\DllOverrides", "/v", dll, "/d", "native", "/f"]);
        wine::run_logged(app, TASK, cmd).await?;
    }

    std::fs::write(cfg.wine_prefix().join(MARKER), &release.tag_name).map_err(|e| e.to_string())?;
    wine::log(app, TASK, format!("DXVK {} instalado", release.tag_name));
    Ok(release.tag_name)
}

/// Copies `x64/*.dll` to system32 and `x32/*.dll` to syswow64.
fn extract(archive: &[u8], prefix: &Path) -> Result<usize, String> {
    let windows = prefix.join("drive_c").join("windows");
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let mut copied = 0;
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let parts: Vec<_> = path
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        // Expected layout: dxvk-<ver>/<arch>/<name>.dll
        let [_, arch, name] = parts.as_slice() else { continue };
        if !name.ends_with(".dll") {
            continue;
        }
        let dest = match arch.as_str() {
            "x64" => windows.join("system32"),
            "x32" => windows.join("syswow64"),
            _ => continue,
        };
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        entry.unpack(dest.join(name)).map_err(|e| e.to_string())?;
        copied += 1;
    }
    Ok(copied)
}
