//! Telling which Dreamcast game a disc image is, to download its box art from
//! libretro-thumbnails, which names games as Redump does (`Crazy Taxi
//! (USA)`): by the serial in the disc's header (its IP.BIN), else by the
//! image's, folder's or header's title. `dreamcast.tsv`, made by
//! `scripts/dreamcast-games.py`, lists the games that have box art, with
//! their serials.

use super::playstation::{self, Entry};
use super::Cover;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// The games with box art: `name<TAB>serial serial…` per line.
const GAMES: &str = include_str!("dreamcast.tsv");

static ENTRIES: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    GAMES
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, serials)| Entry {
            codes: serials.split_whitespace().map(code).collect(),
            name: name.into(),
            ..Default::default()
        })
        .collect()
});

/// A serial as compared: the box's `MK-51064-50` is `MK-5106450` on the disc,
/// and Redump may write `MK-51035` as `51035`.
fn code(serial: &str) -> String {
    let code: String = serial.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_uppercase();
    code.strip_prefix("MK").map(str::to_string).unwrap_or(code)
}

/// How a disc's header starts.
const MAGIC: &[u8] = b"SEGA SEGAKATANA ";
/// Bytes read where a header should be: a raw sector holds it, after the
/// sector's header.
const HEAD: usize = 4096;

/// What a disc's header says of the game.
#[derive(Debug, PartialEq)]
struct Header {
    serial: String,
    /// Which disc it is, when the game has more than one.
    disc: Option<u32>,
    title: String,
}

/// The header in `data`, if it holds one.
fn parse(data: &[u8]) -> Option<Header> {
    let at = data.windows(MAGIC.len()).position(|w| w == MAGIC)?;
    let ip = data.get(at..at + 0x100)?;
    let field = |from: usize, to: usize| String::from_utf8_lossy(&ip[from..to]).trim().to_string();
    // The device information ends in `GD-ROM1/2`.
    let device = field(0x20, 0x30);
    let disc = device.split_once("GD-ROM").and_then(|(_, n)| n.split_once('/')).and_then(|(n, of)| {
        let (n, of) = (n.trim().parse::<u32>().ok()?, of.trim().parse::<u32>().ok()?);
        (of > 1).then_some(n)
    });
    Some(Header { serial: field(0x40, 0x4a), disc, title: field(0x80, 0x100) })
}

/// The tracks a `.gdi` lists, those of the high-density area, where the
/// game is, first. Each line is `number lba type sector-size file offset`,
/// the file quoted if it has spaces.
fn gdi_tracks(gdi: &Path) -> Option<Vec<PathBuf>> {
    let text = std::fs::read_to_string(gdi).ok()?;
    let dir = gdi.parent()?;
    let mut tracks: Vec<(bool, PathBuf)> = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.trim().splitn(5, char::is_whitespace);
            let lba: u32 = parts.nth(1)?.parse().ok()?;
            let rest = parts.nth(2)?.trim_start();
            let file = match rest.strip_prefix('"') {
                Some(quoted) => quoted.split('"').next()?,
                None => rest.split_whitespace().next()?,
            };
            Some((lba < 45000, dir.join(file)))
        })
        .collect();
    tracks.sort_by_key(|(low, _)| *low);
    Some(tracks.into_iter().map(|(_, file)| file).collect())
}

/// The files a `.cue` sheet names, those after `REM HIGH-DENSITY AREA` first.
fn cue_files(cue: &Path) -> Option<Vec<PathBuf>> {
    let sheet = std::fs::read_to_string(cue).ok()?;
    let dir = cue.parent()?;
    let mut high = false;
    let mut files: Vec<(bool, PathBuf)> = Vec::new();
    for line in sheet.lines().map(str::trim) {
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("REM") && upper.contains("HIGH-DENSITY") {
            high = true;
        } else if upper.starts_with("FILE ") {
            let name = match line[5..].trim().strip_prefix('"') {
                Some(quoted) => quoted.split('"').next(),
                None => line[5..].split_whitespace().next(),
            };
            files.extend(name.map(|n| (!high, dir.join(n))));
        }
    }
    files.sort_by_key(|(low, _)| *low);
    Some(files.into_iter().map(|(_, file)| file).collect())
}

/// Reads a DiscJuggler (`.cdi`) descriptor.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let bytes = self.data.get(self.at..self.at + n)?;
        self.at += n;
        Some(bytes)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(drop)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
}

const CDI_V2: u32 = 0x8000_0004;
const CDI_V3: u32 = 0x8000_0005;
const CDI_V35: u32 = 0x8000_0006;
/// How each track's description starts, twice.
const TRACK_MARK: [u8; 10] = [0, 0, 1, 0, 0, 0, 0xff, 0xff, 0xff, 0xff];

/// Where each track's data starts in a `.cdi`, last first: a self-booting
/// disc has the game in its last session, after any audio. The descriptor
/// listing them is at the end of the file, as Flycast reads it.
fn cdi_tracks(cdi: &Path) -> Option<Vec<u64>> {
    let file = File::open(cdi).ok()?;
    let len = file.metadata().ok()?.len();
    let mut tail = [0; 8];
    file.read_exact_at(&mut tail, len.checked_sub(8)?).ok()?;
    let version = u32::from_le_bytes(tail[..4].try_into().ok()?);
    let offset = u32::from_le_bytes(tail[4..].try_into().ok()?) as u64;
    let start = match version {
        CDI_V2 | CDI_V3 => offset,
        CDI_V35 => len.checked_sub(offset)?,
        _ => return None,
    };
    let size = len.checked_sub(start).filter(|s| *s <= 1 << 20)?;
    let mut descriptor = vec![0; size as usize];
    file.read_exact_at(&mut descriptor, start).ok()?;

    let mut r = Reader { data: &descriptor, at: 0 };
    let mut tracks = Vec::new();
    let mut position = 0;
    for _ in 0..r.u16()? {
        for _ in 0..r.u16()? {
            if r.u32()? != 0 {
                r.skip(8)?;
            }
            if r.take(10)? != TRACK_MARK || r.take(10)? != TRACK_MARK {
                return None;
            }
            r.skip(4)?;
            let name = r.u8()? as usize;
            r.skip(name + 11 + 4 + 4)?;
            if r.u32()? == 0x8000_0000 {
                r.skip(8)?;
            }
            r.skip(2)?;
            let pregap = r.u32()? as u64;
            r.skip(4 + 6 + 4 + 12 + 4)?;
            let total = r.u32()? as u64;
            r.skip(16)?;
            let sector = match r.u32()? {
                0 => 2048,
                1 => 2336,
                2 => 2352,
                4 => 2448,
                _ => return None,
            };
            r.skip(29)?;
            if version != CDI_V2 {
                r.skip(5)?;
                if r.u32()? == 0xffff_ffff {
                    r.skip(78)?;
                }
            }
            tracks.push(position + pregap * sector);
            position += total * sector;
        }
        r.skip(4 + 8 + usize::from(version != CDI_V2))?;
    }
    tracks.reverse();
    Some(tracks)
}

/// The start of `file` from `at` on.
fn head(file: &Path, at: u64) -> Option<Vec<u8>> {
    let mut buf = vec![0; HEAD];
    let n = File::open(file).ok()?.read_at(&mut buf, at).ok()?;
    buf.truncate(n);
    Some(buf)
}

/// The header of the disc `rom`. Compressed images (`.chd`) can't be read.
fn header(rom: &Path) -> Option<Header> {
    let places: Vec<(PathBuf, u64)> = match rom.extension()?.to_string_lossy().to_lowercase().as_str() {
        "gdi" => gdi_tracks(rom)?.into_iter().map(|f| (f, 0)).collect(),
        "cue" => cue_files(rom)?.into_iter().map(|f| (f, 0)).collect(),
        "cdi" => cdi_tracks(rom)?.into_iter().map(|at| (rom.to_path_buf(), at)).collect(),
        _ => return None,
    };
    places.iter().find_map(|(file, at)| parse(&head(file, *at)?))
}

/// The game whose serial `header` has: of a game's discs, which share it,
/// the one it is.
fn by_serial(header: &Header) -> Option<&'static Entry> {
    let code = code(&header.serial);
    let found: Vec<&Entry> = ENTRIES.iter().filter(|e| !code.is_empty() && e.codes.contains(&code)).collect();
    let disc = header.disc.map(|n| format!("(Disc {n})"));
    found.iter().find(|e| disc.as_ref().is_some_and(|d| e.name.contains(d.as_str()))).or(found.first()).copied()
}

/// The Redump name of the game `rom`, if it has box art.
fn name(rom: &Path) -> Option<String> {
    let header = header(rom);
    if let Some(entry) = header.as_ref().and_then(by_serial) {
        return Some(entry.name.clone());
    }
    let folder = rom.parent().and_then(Path::file_name);
    let names: Vec<String> = [rom.file_name(), folder]
        .into_iter()
        .flatten()
        .map(|n| n.to_string_lossy().into_owned())
        .chain(header.map(|h| h.title))
        .collect();
    playstation::identify(ENTRIES.iter(), &names).map(|(entry, _)| entry.name.clone())
}

/// The box art of the game `rom`, in libretro-thumbnails.
pub fn cover(rom: &Path) -> Option<Cover> {
    let name = name(rom)?;
    let mut url = reqwest::Url::parse("https://raw.githubusercontent.com/libretro-thumbnails/Sega_-_Dreamcast/master/Named_Boxarts").ok()?;
    url.path_segments_mut().ok()?.push(&format!("{name}.png"));
    Some(Cover { file: format!("dc/{name}.png"), url: url.into(), key: name, lookup: "el nombre del juego" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A header for `serial`, disc `disc`.
    fn ip(serial: &str, disc: &str, title: &str) -> Vec<u8> {
        let mut ip = vec![b' '; 0x100];
        ip[..16].copy_from_slice(MAGIC);
        let device = format!("B882 GD-ROM{disc}");
        ip[0x20..0x20 + device.len()].copy_from_slice(device.as_bytes());
        ip[0x40..0x40 + serial.len()].copy_from_slice(serial.as_bytes());
        ip[0x80..0x80 + title.len()].copy_from_slice(title.as_bytes());
        // As in a raw sector, after its header.
        [vec![0; 16], ip].concat()
    }

    #[test]
    fn reads_headers() {
        let header = parse(&ip("T3601M", "1/1", "DEAD OR ALIVE 2")).unwrap();
        assert_eq!(header, Header { serial: "T3601M".into(), disc: None, title: "DEAD OR ALIVE 2".into() });
        assert_eq!(parse(&ip("MK-51059", "2/4", "SHENMUE")).unwrap().disc, Some(2));
        assert!(parse(b"\0\0 not a header").is_none());
    }

    #[test]
    fn compares_serials_as_written_anywhere() {
        assert_eq!(code("MK-51064-50"), code("MK-5106450"));
        assert_eq!(code("51035"), code("MK-51035"));
        assert_eq!(code("T-3601M"), "T3601M");
    }

    #[test]
    fn finds_games_by_serial() {
        let header = |serial: &str, disc| Header { serial: serial.into(), disc, title: String::new() };
        assert_eq!(by_serial(&header("T3601M", None)).unwrap().name, "Dead or Alive 2 (Japan) (Shokai Genteiban)");
        assert_eq!(by_serial(&header("MK-51035", None)).unwrap().name, "Crazy Taxi (USA)");
        assert_eq!(by_serial(&header("51036", Some(3))).unwrap().name, "D2 (USA) (Disc 3)");
        assert!(by_serial(&header("SEGA-001", None)).is_none());
        assert!(by_serial(&header("", None)).is_none());
    }

    #[test]
    fn finds_games_by_name() {
        let find = |n: &str| playstation::identify(ENTRIES.iter(), &[n.to_string()]).map(|(e, _)| e.name.clone());
        assert_eq!(find("Crazy Taxi (USA).gdi").as_deref(), Some("Crazy Taxi (USA)"));
        assert_eq!(find("Crazy Taxi (Japan) [!].cdi").as_deref(), Some("Crazy Taxi (Japan)"));
        assert_eq!(find("SOULCALIBUR").as_deref(), Some("Soulcalibur (Europe) (En,Fr,De,Es)"));
        assert_eq!(find("shenmue2_disc1.cdi"), None);
    }

    #[test]
    fn reads_track_lists() {
        let dir = std::env::temp_dir().join(format!("runux-dc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let gdi = dir.join("disc.gdi");
        std::fs::write(&gdi, "3\n1 0 4 2352 track01.bin 0\n2 756 0 2352 \"track 02.raw\" 0\n3 45000 4 2352 track03.bin 0\n").unwrap();
        assert_eq!(gdi_tracks(&gdi).unwrap(), [dir.join("track03.bin"), dir.join("track01.bin"), dir.join("track 02.raw")]);
        let cue = dir.join("game.cue");
        std::fs::write(&cue, "FILE \"G (Track 1).bin\" BINARY\nREM HIGH-DENSITY AREA\nFILE \"G (Track 3).bin\" BINARY\n").unwrap();
        assert_eq!(cue_files(&cue).unwrap(), [dir.join("G (Track 3).bin"), dir.join("G (Track 1).bin")]);
        std::fs::write(dir.join("track03.bin"), ip("T-3601M", "1/1", "DEAD OR ALIVE 2")).unwrap();
        assert_eq!(name(&gdi).as_deref(), Some("Dead or Alive 2 (Japan) (Shokai Genteiban)"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn links_to_the_box_art() {
        let cover = cover(Path::new("/g/Crazy Taxi (USA).chd")).unwrap();
        assert_eq!(cover.file, "dc/Crazy Taxi (USA).png");
        assert_eq!(
            cover.url,
            "https://raw.githubusercontent.com/libretro-thumbnails/Sega_-_Dreamcast/master/Named_Boxarts/Crazy%20Taxi%20(USA).png"
        );
    }
}
