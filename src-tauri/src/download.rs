use futures_util::StreamExt;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

pub const USER_AGENT: &str = concat!("runux/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Serialize)]
struct Progress<'a> {
    task: &'a str,
    downloaded: u64,
    total: Option<u64>,
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())
}

/// Downloads `url` into memory, emitting `progress` events along the way.
pub async fn fetch(app: &AppHandle, task: &str, url: &str) -> Result<Vec<u8>, String> {
    let resp = client()?
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Error descargando {url}: {e}"))?;
    let total = resp.content_length();
    let mut data = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut stream = resp.bytes_stream();
    let mut last_emit = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        data.extend_from_slice(&chunk);
        let downloaded = data.len() as u64;
        if downloaded - last_emit > 256 * 1024 || Some(downloaded) == total {
            last_emit = downloaded;
            let _ = app.emit("progress", Progress { task, downloaded, total });
        }
    }
    Ok(data)
}

/// Downloads `url` into the file `dest`, for archives too big to hold in
/// memory, emitting `progress` events along the way.
pub async fn fetch_to_file(app: &AppHandle, task: &str, url: &str, dest: &Path) -> Result<(), String> {
    let resp = client()?
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Error descargando {url}: {e}"))?;
    let total = resp.content_length();
    let mut file = tokio::fs::File::create(dest).await.map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let (mut downloaded, mut last_emit) = (0u64, 0u64);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit > 1024 * 1024 || Some(downloaded) == total {
            last_emit = downloaded;
            let _ = app.emit("progress", Progress { task, downloaded, total });
        }
    }
    file.flush().await.map_err(|e| e.to_string())
}
