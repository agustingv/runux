use crate::config;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub const USER_AGENT: &str = concat!("runux/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Serialize)]
struct Progress<'a> {
    task: &'a str,
    downloaded: u64,
    total: Option<u64>,
}

/// A GitHub release, as `releases/latest` describes it.
#[derive(Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

/// Gives up on a server that stops answering instead of waiting forever. No
/// overall limit: GE-Proton takes minutes to download on a slow line.
pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())
}

async fn get(url: &str) -> Result<reqwest::Response, String> {
    client()?
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Error descargando {url}: {e}"))
}

/// The latest release of a GitHub repository, from its `releases/latest` API URL.
pub async fn latest_release(api_url: &str) -> Result<Release, String> {
    get(api_url).await?.json().await.map_err(|e| format!("Respuesta inesperada de {api_url}: {e}"))
}

pub async fn fetch_text(url: &str) -> Result<String, String> {
    get(url).await?.text().await.map_err(|e| format!("Error descargando {url}: {e}"))
}

/// Streams `url` into `out`, emitting `progress` events along the way.
async fn fetch_into<W: AsyncWrite + Unpin>(app: &AppHandle, task: &str, url: &str, out: &mut W) -> Result<(), String> {
    let resp = get(url).await?;
    let total = resp.content_length();
    let mut stream = resp.bytes_stream();
    let (mut downloaded, mut last_emit) = (0u64, 0u64);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Error descargando {url}: {e}"))?;
        out.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit > 512 * 1024 || Some(downloaded) == total {
            last_emit = downloaded;
            let _ = app.emit("progress", Progress { task, downloaded, total });
        }
    }
    out.flush().await.map_err(|e| e.to_string())
}

/// Downloads `url` into memory.
pub async fn fetch(app: &AppHandle, task: &str, url: &str) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    fetch_into(app, task, url, &mut data).await?;
    Ok(data)
}

/// Downloads `url` into the app's cache as `file`, for installers to run and
/// archives too big to hold in memory; returns its path.
pub async fn fetch_to_cache(app: &AppHandle, task: &str, url: &str, file: &str) -> Result<PathBuf, String> {
    let cache = config::cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let dest = cache.join(file);
    fetch_to_file(app, task, url, &dest).await?;
    Ok(dest)
}

async fn fetch_to_file(app: &AppHandle, task: &str, url: &str, dest: &Path) -> Result<(), String> {
    let mut file = tokio::fs::File::create(dest).await.map_err(|e| format!("{}: {e}", dest.display()))?;
    fetch_into(app, task, url, &mut file).await
}
