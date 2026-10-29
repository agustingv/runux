//! Box art for games whose discs carry no pictures (PS1, PS2). Runux never
//! fetches it on its own: the player downloads each one from the game's page,
//! and from then on it's read from the cache.

use crate::config;
use crate::emulator::Cover;
use std::path::PathBuf;
use std::time::Duration;

/// Biggest cover taken: they're a few hundred KB.
const MAX_SIZE: usize = 8 << 20;

fn path(cover: &Cover) -> PathBuf {
    config::cache_dir().join("covers").join(&cover.file)
}

/// Whether `data` is a JPEG, as the covers are.
fn is_jpeg(data: &[u8]) -> bool {
    data.starts_with(&[0xff, 0xd8, 0xff])
}

/// The cover, if it was downloaded.
pub fn cached(cover: &Cover) -> Option<Vec<u8>> {
    std::fs::read(path(cover)).ok()
}

pub fn is_cached(cover: &Cover) -> bool {
    path(cover).is_file()
}

/// Downloads the cover into the cache.
pub async fn download(cover: &Cover) -> Result<(), String> {
    let unreachable = |e: reqwest::Error| format!("No se pudo descargar la carátula: {e}");
    let response = crate::download::client()?
        .get(&cover.url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(unreachable)?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("La colección de carátulas no tiene la de {}", cover.serial));
    }
    let data = response.error_for_status().map_err(unreachable)?.bytes().await.map_err(unreachable)?;
    if !is_jpeg(&data) || data.len() > MAX_SIZE {
        return Err("Lo descargado no es una carátula".into());
    }
    let path = path(cover);
    let dir = path.parent().ok_or("Ruta de carátula no válida")?;
    let tmp = path.with_extension("part");
    std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(&tmp, &data))
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("No se pudo guardar la carátula: {e}"))
}

/// Deletes the downloaded cover.
pub fn remove(cover: &Cover) -> Result<(), String> {
    match std::fs::remove_file(path(cover)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("No se pudo borrar la carátula: {e}")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_only_jpegs() {
        assert!(is_jpeg(b"\xff\xd8\xff\xe0 JFIF"));
        assert!(!is_jpeg(b"404: Not Found"));
    }
}
