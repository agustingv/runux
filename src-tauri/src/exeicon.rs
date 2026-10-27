//! The icon embedded in a Windows executable, read from its resources
//! without loading the whole file (game executables run to hundreds of MB).

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
/// No icon image comes close; guards against corrupt sizes.
const MAX_READ: u32 = 8 << 20;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

pub struct Icon {
    pub mime: &'static str,
    pub data: Vec<u8>,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

struct Section {
    rva: u32,
    size: u32,
    offset: u32,
}

struct Pe {
    file: File,
    sections: Vec<Section>,
    /// RVA of the resource directory.
    rsrc: u32,
}

impl Pe {
    fn open(path: &Path) -> Option<Pe> {
        let file = File::open(path).ok()?;
        let read = |at: u64, len: usize| {
            let mut buf = vec![0; len];
            file.read_exact_at(&mut buf, at).ok().map(|_| buf)
        };
        let dos = read(0, 64)?;
        if !dos.starts_with(b"MZ") {
            return None;
        }
        let pe = u64::from(u32_at(&dos, 0x3c)?);
        let coff = read(pe, 24)?;
        if !coff.starts_with(b"PE\0\0") {
            return None;
        }
        let section_count = usize::from(u16_at(&coff, 6)?);
        let optional_size = usize::from(u16_at(&coff, 20)?);
        let optional = read(pe + 24, optional_size)?;
        // The data directories follow the optional header's fixed fields,
        // which are longer in 64-bit executables; resources are the third.
        let directories = match u16_at(&optional, 0)? {
            0x10b => 96,
            0x20b => 112,
            _ => return None,
        };
        let rsrc = u32_at(&optional, directories + 2 * 8)?;
        if rsrc == 0 {
            return None;
        }
        let table = read(pe + 24 + optional_size as u64, section_count * 40)?;
        let sections = table
            .chunks_exact(40)
            .filter_map(|s| {
                Some(Section {
                    size: u32_at(s, 8)?.max(u32_at(s, 16)?),
                    rva: u32_at(s, 12)?,
                    offset: u32_at(s, 20)?,
                })
            })
            .collect();
        Some(Pe { file, sections, rsrc })
    }

    fn read_rva(&self, rva: u32, len: u32) -> Option<Vec<u8>> {
        if len > MAX_READ {
            return None;
        }
        let s = self.sections.iter().find(|s| rva >= s.rva && rva - s.rva < s.size)?;
        let mut buf = vec![0; len as usize];
        self.file
            .read_exact_at(&mut buf, u64::from(s.offset) + u64::from(rva - s.rva))
            .ok()?;
        Some(buf)
    }

    /// Entries of the resource directory at `dir` (relative to the
    /// resource section) as (numeric id, offset with its subdirectory flag).
    fn entries(&self, dir: u32) -> Option<Vec<(Option<u32>, u32)>> {
        let head = self.read_rva(self.rsrc + dir, 16)?;
        let count = u32::from(u16_at(&head, 12)?) + u32::from(u16_at(&head, 14)?);
        let raw = self.read_rva(self.rsrc + dir + 16, count * 8)?;
        Some(
            raw.chunks_exact(8)
                .filter_map(|e| {
                    let name = u32_at(e, 0)?;
                    Some(((name & 0x8000_0000 == 0).then_some(name), u32_at(e, 4)?))
                })
                .collect(),
        )
    }

    /// The resource of `kind` named `id` (the first one if `None`), in
    /// whichever language comes first.
    fn resource(&self, kind: u32, id: Option<u32>) -> Option<Vec<u8>> {
        let subdir = |entry: &(Option<u32>, u32)| (entry.1 & 0x8000_0000 != 0).then_some(entry.1 & 0x7fff_ffff);
        let types = self.entries(0)?;
        let names = self.entries(subdir(types.iter().find(|e| e.0 == Some(kind))?)?)?;
        let name = match id {
            Some(id) => names.iter().find(|e| e.0 == Some(id))?,
            None => names.first()?,
        };
        let leaf = self.entries(subdir(name)?)?.first()?.1;
        let data = self.read_rva(self.rsrc + leaf, 8)?;
        self.read_rva(u32_at(&data, 0)?, u32_at(&data, 4)?)
    }
}

/// The largest image of the executable's main icon: a PNG as is, a bitmap
/// wrapped in a one-image `.ico`.
pub fn read(exe: &Path) -> Option<Icon> {
    let pe = Pe::open(exe)?;
    let group = pe.resource(RT_GROUP_ICON, None)?;
    let count = usize::from(u16_at(&group, 4)?);
    // 14-byte entries: width and height (0 meaning 256), colors, reserved,
    // planes, bit count, size and the RT_ICON id.
    let entry = (0..count)
        .filter_map(|i| group.get(6 + i * 14..6 + (i + 1) * 14))
        .max_by_key(|e| (if e[0] == 0 { 256 } else { u16::from(e[0]) }, u16_at(e, 6)))?;
    let image = pe.resource(RT_ICON, Some(u32::from(u16_at(entry, 12)?)))?;
    if image.starts_with(PNG_SIGNATURE) {
        return Some(Icon { mime: "image/png", data: image });
    }
    let mut ico = vec![0, 0, 1, 0, 1, 0];
    ico.extend_from_slice(&entry[..8]);
    ico.extend_from_slice(&(image.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(&image);
    Some(Icon { mime: "image/x-icon", data: ico })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_files_that_are_not_executables() {
        let path = std::env::temp_dir().join(format!("runux-icon-{}.exe", std::process::id()));
        std::fs::write(&path, b"MZ but nothing else").unwrap();
        assert!(read(&path).is_none());
        std::fs::remove_file(&path).unwrap();
        assert!(read(&path).is_none());
    }
}
