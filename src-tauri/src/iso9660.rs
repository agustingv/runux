//! Reading a file from an ISO 9660 image without mounting it: enough to take
//! a game's pictures from a PS3 or PSP disc image.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;

const SECTOR: u64 = 2048;
/// Biggest directory read: a disc's folders hold a few dozen entries.
const MAX_DIR: u32 = 1 << 20;

/// Where a file or folder is in the image, and its size.
#[derive(Clone, Copy)]
struct Extent {
    sector: u32,
    size: u32,
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn read(file: &File, extent: Extent, max: u32) -> Option<Vec<u8>> {
    if extent.size > max {
        return None;
    }
    let mut buf = vec![0; extent.size as usize];
    file.read_exact_at(&mut buf, extent.sector as u64 * SECTOR).ok()?;
    Some(buf)
}

/// The entry `name` of a directory's records, ignoring case and the `;1`
/// version suffix.
fn find(dir: &[u8], name: &str) -> Option<Extent> {
    let mut at = 0;
    while at < dir.len() {
        let len = dir[at] as usize;
        if len == 0 {
            // Records don't cross sectors: the rest of this one is padding.
            at = (at / SECTOR as usize + 1) * SECTOR as usize;
            continue;
        }
        let record = dir.get(at..at + len)?;
        let name_len = *record.get(32)? as usize;
        let raw = record.get(33..33 + name_len)?;
        let entry = String::from_utf8_lossy(raw);
        let entry = entry.split(';').next().unwrap_or_default().trim_end_matches('.');
        if entry.eq_ignore_ascii_case(name) {
            return Some(Extent { sector: u32_at(record, 2)?, size: u32_at(record, 10)? });
        }
        at += len;
    }
    None
}

/// The file at `path` (`PS3_GAME/ICON0.PNG`) in the image `iso`, if it's
/// there and no bigger than `max` bytes.
pub fn read_file(iso: &Path, path: &str, max: u32) -> Option<Vec<u8>> {
    let file = File::open(iso).ok()?;
    // The primary volume descriptor, in sector 16, holds the root's record.
    let mut pvd = [0; SECTOR as usize];
    file.read_exact_at(&mut pvd, 16 * SECTOR).ok()?;
    if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
        return None;
    }
    let mut extent = Extent { sector: u32_at(&pvd, 156 + 2)?, size: u32_at(&pvd, 156 + 10)? };
    for name in path.split('/') {
        extent = find(&read(&file, extent, MAX_DIR)?, name)?;
    }
    read(&file, extent, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory record pointing at `sector`.
    fn record(name: &[u8], sector: u32, size: u32) -> Vec<u8> {
        let len = 33 + name.len() + (name.len() + 1) % 2;
        let mut r = vec![0; len];
        r[0] = len as u8;
        r[2..6].copy_from_slice(&sector.to_le_bytes());
        r[10..14].copy_from_slice(&size.to_le_bytes());
        r[32] = name.len() as u8;
        r[33..33 + name.len()].copy_from_slice(name);
        r
    }

    /// An image with `PSP_GAME/ICON0.PNG`, whose folder spills its last
    /// record into a second sector.
    fn image() -> Vec<u8> {
        let mut iso = vec![0; 22 * SECTOR as usize];
        let sector = |n: u32| n as usize * SECTOR as usize;
        iso[sector(16)] = 1;
        iso[sector(16) + 1..sector(16) + 6].copy_from_slice(b"CD001");
        let root = record(b"\0", 18, SECTOR as u32);
        iso[sector(16) + 156..sector(16) + 156 + root.len()].copy_from_slice(&root);
        let mut dir = [record(b"\0", 18, 2048), record(b"\x01", 18, 2048), record(b"PSP_GAME", 19, 2 * 2048)].concat();
        iso[sector(18)..sector(18) + dir.len()].copy_from_slice(&dir);
        dir = [record(b"\0", 19, 4096), record(b"PARAM.SFO;1", 21, 3)].concat();
        iso[sector(19)..sector(19) + dir.len()].copy_from_slice(&dir);
        let icon = record(b"ICON0.PNG;1", 21, 4);
        iso[sector(20)..sector(20) + icon.len()].copy_from_slice(&icon);
        iso[sector(21)..sector(21) + 4].copy_from_slice(b"\x89PNG");
        iso
    }

    #[test]
    fn reads_files_by_path() {
        let path = std::env::temp_dir().join(format!("runux-iso-{}.iso", std::process::id()));
        std::fs::write(&path, image()).unwrap();
        assert_eq!(read_file(&path, "PSP_GAME/ICON0.PNG", 1024).as_deref(), Some(&b"\x89PNG"[..]));
        assert_eq!(read_file(&path, "psp_game/icon0.png", 1024).as_deref(), Some(&b"\x89PNG"[..]));
        assert_eq!(read_file(&path, "PSP_GAME/ICON0.PNG", 3), None);
        assert_eq!(read_file(&path, "PSP_GAME/PIC1.PNG", 1024), None);
        assert_eq!(read_file(&path, "PS3_GAME/ICON0.PNG", 1024), None);
        std::fs::write(&path, b"not an image").unwrap();
        assert_eq!(read_file(&path, "PSP_GAME/ICON0.PNG", 1024), None);
        std::fs::remove_file(&path).unwrap();
    }
}
