//! Box art for games whose discs carry no pictures (PS1, PS2), downloaded
//! once and kept in the cache.

use crate::config;
use crate::emulator::Cover;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// How long a cover the collection lacks isn't asked for again.
const RETRY_MISSING: Duration = Duration::from_secs(7 * 24 * 3600);
/// Biggest cover taken: they're a few hundred KB.
const MAX_SIZE: usize = 8 << 20;

fn path(cover: &Cover) -> PathBuf {
    config::cache_dir().join("covers").join(&cover.file)
}

/// Whether `data` is a JPEG, as the covers are.
fn is_jpeg(data: &[u8]) -> bool {
    data.starts_with(&[0xff, 0xd8, 0xff])
}

/// The cover, from the cache or downloaded into it. `None` when the
/// collection has none (remembered for a week) or it can't be reached.
pub async fn get(cover: &Cover) -> Option<Vec<u8>> {
    let path = path(cover);
    if let Ok(data) = tokio::fs::read(&path).await {
        return Some(data);
    }
    let missing = path.with_extension("missing");
    let modified = std::fs::metadata(&missing).and_then(|m| m.modified()).ok();
    if modified.is_some_and(|m| SystemTime::now().duration_since(m).unwrap_or_default() < RETRY_MISSING) {
        return None;
    }
    let response = crate::download::client().ok()?.get(&cover.url).timeout(Duration::from_secs(30)).send().await.ok()?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        let _ = std::fs::write(&missing, "");
        return None;
    }
    let data = response.error_for_status().ok()?.bytes().await.ok()?;
    if !is_jpeg(&data) || data.len() > MAX_SIZE {
        return None;
    }
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &data).and_then(|()| std::fs::rename(&tmp, &path)).ok()?;
    Some(data.to_vec())
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
