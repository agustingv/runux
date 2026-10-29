//! Telling which PlayStation or PlayStation 2 game a disc image is, by its
//! serial (`SLES-02529`): from the disc's `SYSTEM.CNF`, the file's name, or
//! its title in the emulator's game list. DuckStation and PCSX2 share it.

use super::program::Install;
use std::path::{Path, PathBuf};

/// The serials (`SLES-02529`) in a text, which may write them `sles_025.29`,
/// as on the disc.
pub fn serials(text: &str) -> Vec<String> {
    let b = text.to_ascii_uppercase().into_bytes();
    let mut found = Vec::new();
    for i in 0..b.len() {
        let rest = &b[i..];
        if (i > 0 && b[i - 1].is_ascii_alphanumeric()) || rest.len() < 9 || !rest[..4].iter().all(u8::is_ascii_uppercase) {
            continue;
        }
        let mut k = if matches!(rest[4], b'-' | b'_' | b' ') { 5 } else { 4 };
        let mut digits = String::new();
        while digits.len() < 5 {
            match rest.get(k) {
                Some(c) if c.is_ascii_digit() => digits.push(*c as char),
                Some(b'.') if digits.len() == 3 => {}
                _ => break,
            }
            k += 1;
        }
        if digits.len() == 5 && !rest.get(k).is_some_and(u8::is_ascii_digit) {
            found.push(format!("{}-{digits}", String::from_utf8_lossy(&rest[..4])));
        }
    }
    found
}

/// A file name without its extensions: `Game.bin.ecm` → `Game`, but
/// `Dr. Mario.cue` → `Dr. Mario`.
pub fn title(name: &str) -> &str {
    let mut title = name;
    while let Some((stem, ext)) = title.rsplit_once('.') {
        if ext.is_empty() || ext.len() > 4 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            break;
        }
        title = stem;
    }
    title.trim()
}

/// `title` without its `(…)` and `[…]` parts: `Game (Europe) [SLES-1]` → `Game`.
fn bare(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth as i32 - 1).max(0) as u32,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The region a title or region field names, as PCSX2 writes it: `PAL`,
/// `NTSC-U` or `NTSC-J`.
fn region(text: &str) -> Option<&'static str> {
    let t = text.to_lowercase();
    if ["(europe", "(e)", "(spain", "(france", "(germany", "(italy", "(uk", "pal"].iter().any(|r| t.contains(r)) {
        Some("PAL")
    } else if ["(usa", "(u)", "ntsc-u"].iter().any(|r| t.contains(r)) {
        Some("NTSC-U")
    } else if ["(japan", "(j)", "ntsc-j"].iter().any(|r| t.contains(r)) {
        Some("NTSC-J")
    } else {
        None
    }
}

/// A game in DuckStation's `gamedb.yaml` or PCSX2's `GameIndex.yaml`.
#[derive(Debug, Default, PartialEq)]
pub struct Entry {
    /// Its serial, and any others under `codes:`.
    pub codes: Vec<String>,
    pub name: String,
    /// PCSX2's `region:`; DuckStation puts it in the name.
    pub region: String,
    /// Whether DuckStation marks it as protected with LibCrypt.
    pub libcrypt: bool,
}

/// The games in a game list: entries are `SERIAL:` with fields indented
/// two spaces, and more serials listed under `codes:`.
pub fn entries(db: &str) -> Vec<Entry> {
    let mut list: Vec<Entry> = Vec::new();
    let mut in_codes = false;
    for line in db.lines() {
        if !line.starts_with([' ', '#']) && line.ends_with(':') {
            list.push(Entry { codes: vec![line.trim_end_matches(':').to_string()], ..Default::default() });
        } else if let Some(entry) = list.last_mut() {
            if let Some(field) = line.strip_prefix("  ").filter(|f| !f.starts_with(' ')) {
                let (key, value) = field.split_once(':').unwrap_or((field, ""));
                let value = value.trim().trim_matches('"');
                in_codes = key == "codes";
                match key {
                    "name" => entry.name = value.into(),
                    "region" => entry.region = value.into(),
                    "libcrypt" => entry.libcrypt = value == "true",
                    _ => {}
                }
            } else if let Some(code) = line.trim().strip_prefix("- ").filter(|_| in_codes) {
                entry.codes.push(code.trim().to_string());
            }
        }
    }
    list
}

/// The entry of `entries` that `names` (file or folder names, or serials)
/// stand for, and the serial matched: one in a name first, then a title equal
/// to an entry's name, then the same title without regions or tags,
/// preferring the region the name says.
pub fn identify<'a>(entries: impl IntoIterator<Item = &'a Entry>, names: &[String]) -> Option<(&'a Entry, String)> {
    let wanted: Vec<String> = names.iter().flat_map(|n| serials(n)).collect();
    let titles: Vec<&str> = names.iter().map(|n| title(n)).collect();
    let bares: Vec<String> = titles.iter().map(|t| bare(t)).collect();
    let hint = titles.iter().find_map(|t| region(t));
    // Best match so far, ranked: 0 a serial, 1 the exact title, 2 the title
    // and region, 3 the title alone.
    let mut best: Option<(u8, &Entry, String)> = None;
    for entry in entries {
        let rank = if let Some(code) = entry.codes.iter().find(|c| wanted.contains(c)) {
            return Some((entry, code.clone()));
        } else if titles.iter().any(|t| t.eq_ignore_ascii_case(&entry.name)) {
            1
        } else if !entry.name.is_empty() && bares.contains(&bare(&entry.name)) {
            let own = if entry.region.is_empty() { region(&entry.name) } else { region(&entry.region) };
            if hint.is_some() && own == hint { 2 } else { 3 }
        } else {
            continue;
        };
        if best.as_ref().is_none_or(|(r, ..)| rank < *r) {
            best = Some((rank, entry, entry.codes[0].clone()));
        }
    }
    best.map(|(_, entry, serial)| (entry, serial))
}

/// The program's game list `file`, next to its binary or in the Flatpak.
pub fn read_db(install: &Install, file: &str) -> Option<String> {
    let dir = install.files()?;
    [format!("bin/resources/{file}"), format!("resources/{file}")]
        .iter()
        .find_map(|f| std::fs::read_to_string(dir.join(f)).ok())
}

/// The serial of the game `SYSTEM.CNF` boots: `BOOT2 = cdrom0:\SLUS_200.62;1`
/// on a PS2, `BOOT = cdrom:\SLES_025.29;1` on a PS1.
fn boot_serial(cnf: &str) -> Option<String> {
    cnf.lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(key, _)| matches!(key.trim().to_ascii_uppercase().as_str(), "BOOT" | "BOOT2"))
        .find_map(|(_, value)| serials(value.rsplit(['\\', ':']).next()?).into_iter().next())
}

/// The image a `.cue` sheet's first `FILE` line names, next to it.
fn cue_image(cue: &Path) -> Option<PathBuf> {
    let sheet = std::fs::read_to_string(cue).ok()?;
    let line = sheet.lines().map(str::trim).find(|l| l.to_ascii_uppercase().starts_with("FILE "))?;
    let name = match line[5..].trim().strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => line[5..].split_whitespace().next()?,
    };
    Some(cue.parent()?.join(name))
}

/// The serial on the disc `rom`, from its `SYSTEM.CNF`. Only uncompressed
/// images can be read.
pub fn disc_serial(rom: &Path) -> Option<String> {
    let ext = rom.extension()?.to_string_lossy().to_lowercase();
    let image = match ext.as_str() {
        "cue" => cue_image(rom)?,
        "iso" | "bin" | "img" => rom.to_path_buf(),
        _ => return None,
    };
    let cnf = crate::iso9660::read_file(&image, "SYSTEM.CNF", 4096)?;
    boot_serial(&String::from_utf8_lossy(&cnf))
}

/// The names that tell which game `rom` is: its serial, if the disc says it,
/// and the image's and folder's names.
pub fn names(rom: &Path) -> Vec<String> {
    let folder = rom.parent().and_then(Path::file_name);
    disc_serial(rom)
        .into_iter()
        .chain([rom.file_name(), folder].into_iter().flatten().map(|n| n.to_string_lossy().into_owned()))
        .collect()
}

/// The serial of the game `rom`: from the disc or its name, else from its
/// title in the game list `db`.
pub fn serial(rom: &Path, db: Option<&str>) -> Option<String> {
    let names = names(rom);
    if let Some(serial) = names.iter().flat_map(|n| serials(n)).next() {
        return Some(serial);
    }
    identify(&entries(db?), &names).map(|(_, serial)| serial)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_serials_from_names() {
        assert_eq!(serials("Resident Evil 3 - Nemesis (E) [SLES-02529].bin.ecm"), ["SLES-02529"]);
        assert_eq!(serials("sces_015.64.bin"), ["SCES-01564"]);
        assert!(serials("Final Fantasy VII (Disc 1).cue").is_empty());
        assert!(serials("XSLES-025290").is_empty());
    }

    #[test]
    fn strips_extensions_but_not_dots_in_titles() {
        assert_eq!(title("Game (Europe).bin.ecm"), "Game (Europe)");
        assert_eq!(title("Dr. Mario.cue"), "Dr. Mario");
        assert_eq!(bare("Ape Escape (Europe) (En,Fr) [SCES-01564]"), "ape escape");
    }

    #[test]
    fn reads_the_boot_file() {
        assert_eq!(boot_serial("BOOT2 = cdrom0:\\SLUS_200.62;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n").as_deref(), Some("SLUS-20062"));
        assert_eq!(boot_serial("BOOT = cdrom:\\SLES_025.29;1\nTCB = 4\n").as_deref(), Some("SLES-02529"));
        assert_eq!(boot_serial("BOOT = cdrom:\\PSX.EXE;1\n"), None);
    }

    #[test]
    fn finds_the_image_of_a_cue() {
        let dir = std::env::temp_dir().join(format!("runux-cue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.cue"), "FILE \"Game (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n").unwrap();
        assert_eq!(cue_image(&dir.join("a.cue")), Some(dir.join("Game (Track 1).bin")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    const DUCKSTATION: &str = "\
SLES-02529:
  name: \"Resident Evil 3 - Nemesis (Europe)\"
  traits:
    - DisableWidescreen
  libcrypt: true
SCES-01564:
  name: \"Ape Escape (Europe)\"
  codes:
    - SCES-01564
    - SCES-01999
  metadata:
    languages:
      - English
";

    const PCSX2: &str = "\
# PCSX2 Game Database
SLPM-65115:
  name: \"Grand Theft Auto III\"
  region: \"NTSC-J\"
SLES-50330:
  name: \"Grand Theft Auto III\"
  region: \"PAL-M5\"
SLUS-20062:
  name: \"Grand Theft Auto III\"
  region: \"NTSC-U\"
  compat: 5
";

    fn find(db: &str, name: &str) -> Option<String> {
        identify(&entries(db), &[name.to_string()]).map(|(_, serial)| serial)
    }

    #[test]
    fn reads_both_game_lists() {
        let ds = entries(DUCKSTATION);
        assert_eq!(ds.len(), 2);
        assert!(ds[0].libcrypt);
        assert_eq!(ds[1].codes, ["SCES-01564", "SCES-01564", "SCES-01999"]);
        let pcsx2 = entries(PCSX2);
        assert_eq!(pcsx2.len(), 3);
        assert_eq!(pcsx2[1].region, "PAL-M5");
    }

    #[test]
    fn identifies_games_by_serial_or_title() {
        assert_eq!(find(DUCKSTATION, "Ape [SCES-01999].chd").as_deref(), Some("SCES-01999"));
        assert_eq!(find(DUCKSTATION, "Ape Escape (Europe).chd").as_deref(), Some("SCES-01564"));
        assert_eq!(find(DUCKSTATION, "Ape Escape (Europe) (En,Fr,De).chd").as_deref(), Some("SCES-01564"));
        assert_eq!(find(PCSX2, "Grand Theft Auto III (Europe).iso").as_deref(), Some("SLES-50330"));
        assert_eq!(find(PCSX2, "Grand Theft Auto III (USA).chd").as_deref(), Some("SLUS-20062"));
        assert_eq!(find(PCSX2, "Grand Theft Auto III.cso").as_deref(), Some("SLPM-65115"));
        assert_eq!(find(PCSX2, "Grand Theft Auto Vice City.iso"), None);
    }
}
