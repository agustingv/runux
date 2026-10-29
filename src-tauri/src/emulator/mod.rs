//! Emulators: the programs that run games of other systems (Dreamcast,
//! PlayStation…) or engines (ScummVM) natively, without Wine. Each says how
//! it's installed, what it opens, how it starts a game and which of its
//! settings fix known problems, with its help page, so the UI shows each
//! one's own options without knowing it.
//!
//! Adding one takes a `Kind` variant, a module implementing `Emulator`, and
//! its entry in `ALL`.

mod duckstation;
mod flycast;
mod pcsx2;
mod playstation;
mod ppsspp;
mod program;
mod rpcs3;
mod scummvm;

use crate::external::{ExternalGame, Kind};
use crate::help::{list, section, steps, text, HelpSection};
use program::Install;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use ts_rs::TS;

pub use scummvm::detect as detect_scummvm;

/// Every emulator, in the order the sidebar lists their systems.
static ALL: &[&dyn Emulator] = &[
    &scummvm::Scummvm,
    &flycast::Flycast,
    &duckstation::Duckstation,
    &pcsx2::Pcsx2,
    &rpcs3::Rpcs3,
    &ppsspp::Ppsspp,
];

/// What the player picks when adding a game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, rename = "EmulatorMedia")]
pub enum Media {
    /// A file (a disc image), opened from where it is.
    #[serde(rename_all = "camelCase")]
    File {
        /// What it is, e.g. `Imagen del disco`.
        label: String,
        /// Which files and how they're used.
        help: String,
        extensions: Vec<String>,
        /// Whether a game folder also works.
        folder: bool,
    },
    /// The game's ISO images, extracted into the app's folder, or its
    /// folder, used as is; the emulator tells which game it is.
    GameFiles,
}

impl Media {
    /// A disc image of one of `extensions` that `program` opens.
    fn image(program: &str, extensions: &[&str]) -> Media {
        let names: Vec<String> = extensions.iter().map(|e| format!(".{e}")).collect();
        let list = match names.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("Un {} o {last}", rest.join(", ")),
            _ => format!("Un {}", names.join("")),
        };
        Media::File {
            label: "Imagen del disco".into(),
            help: format!("{list}. Se abre con {program} desde donde está; no se copia."),
            extensions: extensions.iter().map(|e| e.to_string()).collect(),
            folder: false,
        }
    }
}

/// A known problem and the setting that fixes it.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "EmulatorFix")]
pub struct Fix {
    pub symptom: String,
    pub explanation: String,
    /// The setting, as the emulator names it on its command line.
    pub key: String,
    /// Empty for a setting that is just a flag.
    pub value: String,
    /// Whether new games start with it.
    pub default: bool,
}

impl Fix {
    /// A command-line flag, off unless chosen.
    fn flag(key: &str, symptom: &str, explanation: &str) -> Fix {
        Fix { symptom: symptom.into(), explanation: explanation.into(), key: key.into(), value: String::new(), default: false }
    }
}

/// A picture a game carries for the console's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// Its icon or logo: PS3 and PSP ones are wider than tall.
    Icon,
    /// The full-screen picture shown behind it.
    Background,
}

/// Box art the player can download for a game whose disc carries no
/// pictures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cover {
    /// The game's serial, which it's looked up by.
    pub serial: String,
    pub url: String,
    /// Its file in the covers' cache: `<system>/<serial>.jpg`.
    pub file: String,
}

impl Cover {
    /// The front cover of `serial` in xlenore's collection `repo`, the one
    /// DuckStation and PCSX2 users point their cover downloaders at.
    fn xlenore(repo: &str, system: &str, serial: &str) -> Cover {
        Cover {
            serial: serial.into(),
            url: format!("https://raw.githubusercontent.com/xlenore/{repo}/main/covers/default/{serial}.jpg"),
            file: format!("{system}/{serial}.jpg"),
        }
    }
}

/// Biggest picture read: a PS3 background is a few MB.
const MAX_ART: u32 = 16 << 20;

/// `data` if it's a PNG, the only kind the UI is given.
fn png(data: Vec<u8>) -> Option<Vec<u8>> {
    data.starts_with(b"\x89PNG\r\n\x1a\n").then_some(data)
}

/// The PNG at `path` in a disc image.
fn iso_art(iso: &Path, path: &str) -> Option<Vec<u8>> {
    png(crate::iso9660::read_file(iso, path, MAX_ART)?)
}

/// The PNG file at `path`.
fn file_art(path: &Path) -> Option<Vec<u8>> {
    let size = std::fs::metadata(path).ok()?.len();
    png(std::fs::read(path).ok().filter(|_| size <= MAX_ART as u64)?)
}

/// Whether `rom` is an `.iso` image.
fn is_iso(rom: &Path) -> bool {
    rom.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso"))
}

/// An emulator as the UI sees it.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EmulatorInfo {
    pub kind: Kind,
    /// What its games are listed under, e.g. `Dreamcast`.
    pub system: String,
    pub program: String,
    pub installed: bool,
    /// The command that installs it, when known.
    pub install_hint: Option<String>,
    /// What to set up in it before playing (a BIOS, firmware…), if anything.
    pub setup: Option<String>,
    pub media: Media,
    pub fixes: Vec<Fix>,
    /// Its help page: setting it up, what to add, known problems.
    pub help: Vec<HelpSection>,
}

pub trait Emulator: Sync {
    fn kind(&self) -> Kind;
    fn system(&self) -> &'static str;
    fn program(&self) -> &'static str;
    fn find(&self) -> Option<Install>;
    /// The command that installs it on this system, when known.
    fn install_hint(&self) -> Option<String>;
    fn media(&self) -> Media;
    /// The command that runs `game` with `settings`.
    fn command(&self, game: &ExternalGame, settings: &BTreeMap<String, String>) -> Result<Command, String>;

    /// What to set up in it before playing (a BIOS, firmware…), if anything.
    fn setup(&self) -> Option<String> {
        None
    }

    fn fixes(&self) -> Vec<Fix> {
        Vec::new()
    }

    fn help(&self) -> Vec<HelpSection> {
        Vec::new()
    }

    /// Rejects a setting the emulator couldn't take on its command line. By
    /// default settings are `--options`.
    fn check_setting(&self, key: &str, value: &str) -> Result<(), String> {
        check_option(self.program(), "--", key, value)
    }

    /// What the command line of the running game contains: by default its
    /// disc image. Command lines end in a space, standing for each
    /// argument's NUL, and so must this, so that `/g/Alive` doesn't match
    /// `/g/Alive2`.
    fn running_marker(&self, game: &ExternalGame) -> String {
        let rom = game.rom.as_deref().unwrap_or(&game.path);
        format!("{} ", rom.to_string_lossy().to_lowercase())
    }

    /// The picture `which` of the game `rom` opens, as a PNG, if its disc or
    /// folder has one.
    fn art(&self, _rom: &Path, _which: Art) -> Option<Vec<u8>> {
        None
    }

    /// Where the player can download the box art of the game `rom` opens,
    /// for systems whose discs carry no pictures.
    fn cover(&self, _rom: &Path) -> Option<Cover> {
        None
    }

    /// The name `rom` (an image or game folder) suggests for the game.
    fn game_name(&self, rom: &Path) -> Option<String> {
        image_name(rom)
    }

    /// The folder holding the game `rom` opens: the one it's in, or itself.
    fn game_dir(&self, rom: &Path) -> PathBuf {
        if rom.is_dir() { rom.to_path_buf() } else { rom.parent().unwrap_or(rom).to_path_buf() }
    }

    fn installed(&self) -> bool {
        self.find().is_some()
    }

    /// Why it can't run anything: it isn't installed.
    fn missing(&self) -> String {
        match self.install_hint() {
            Some(command) => format!("{} no está instalado ({command})", self.program()),
            None => format!("{} no está instalado", self.program()),
        }
    }

    fn require(&self) -> Result<Install, String> {
        self.find().ok_or_else(|| self.missing())
    }

    /// The emulator on its own, to set it up.
    fn open(&self) -> Result<Command, String> {
        Ok(self.require()?.command(&[]))
    }

    /// The settings of a game no one has changed them for.
    fn default_settings(&self) -> BTreeMap<String, String> {
        self.fixes().into_iter().filter(|f| f.default).map(|f| (f.key, f.value)).collect()
    }

    /// `game`'s settings, or the defaults when they were never changed.
    fn settings(&self, game: &ExternalGame) -> BTreeMap<String, String> {
        game.settings.clone().unwrap_or_else(|| self.default_settings())
    }

    fn info(&self) -> EmulatorInfo {
        EmulatorInfo {
            kind: self.kind(),
            system: self.system().into(),
            program: self.program().into(),
            installed: self.installed(),
            install_hint: self.install_hint(),
            setup: self.setup(),
            media: self.media(),
            fixes: self.fixes(),
            help: self.help(),
        }
    }
}

/// The emulator that runs `kind` games; `None` for Windows games.
pub fn get(kind: Kind) -> Option<&'static dyn Emulator> {
    ALL.iter().copied().find(|e| e.kind() == kind)
}

pub fn infos() -> Vec<EmulatorInfo> {
    ALL.iter().map(|e| e.info()).collect()
}

/// The disc image or game folder a game opens.
fn rom(game: &ExternalGame) -> Result<&Path, String> {
    game.rom.as_deref().ok_or_else(|| "Falta la imagen del juego".into())
}

/// The name an image suggests: its file name, unless that's the generic
/// `disc.gdi` of GD-ROM dumps, where the folder names the game. A folder's
/// is its name.
pub fn image_name(rom: &Path) -> Option<String> {
    if rom.is_dir() {
        return Some(rom.file_name()?.to_string_lossy().into());
    }
    let stem = rom.file_stem()?.to_string_lossy();
    if stem.eq_ignore_ascii_case("disc") {
        return Some(rom.parent()?.file_name()?.to_string_lossy().into());
    }
    Some(stem.into())
}

/// A setting part made only of letters, digits and `extra`.
fn is_word(s: &str, extra: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

/// Settings that are command-line options: `-flag`, `--flag` or
/// `--name=value`, written as the key says.
fn option_args(settings: &BTreeMap<String, String>) -> impl Iterator<Item = String> + '_ {
    settings.iter().map(|(k, v)| if v.is_empty() { k.clone() } else { format!("{k}={v}") })
}

/// Accepts an option setting written with `prefix` (`-` or `--`).
fn check_option(program: &str, prefix: &str, key: &str, value: &str) -> Result<(), String> {
    match key.strip_prefix(prefix) {
        Some(name) if is_word(name, "-") && !name.starts_with('-') && !value.contains(char::is_whitespace) => Ok(()),
        _ => Err(format!("Ajuste de {program} no válido: «{key}»")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_but_windows_has_one_emulator() {
        let kinds = [Kind::Scummvm, Kind::Dreamcast, Kind::Playstation, Kind::Playstation2, Kind::Playstation3, Kind::Psp];
        for kind in kinds {
            assert_eq!(ALL.iter().filter(|e| e.kind() == kind).count(), 1, "{kind:?}");
        }
        assert_eq!(ALL.len(), kinds.len());
        assert!(get(Kind::Windows).is_none());
    }

    #[test]
    fn fixes_are_valid_settings() {
        for emu in ALL {
            for fix in emu.fixes() {
                assert!(emu.check_setting(&fix.key, &fix.value).is_ok(), "{}: {}", emu.program(), fix.key);
            }
        }
    }

    #[test]
    fn every_emulator_has_help() {
        for emu in ALL {
            let help = emu.help();
            assert!(!help.is_empty(), "{}", emu.program());
            crate::help::check(&help);
        }
    }

    #[test]
    fn describes_images() {
        let Media::File { help, .. } = Media::image("Flycast", &["gdi", "cdi", "chd"]) else { unreachable!() };
        assert_eq!(help, "Un .gdi, .cdi o .chd. Se abre con Flycast desde donde está; no se copia.");
        let Media::File { help, .. } = Media::image("X", &["iso"]) else { unreachable!() };
        assert!(help.starts_with("Un .iso. "));
    }

    #[test]
    fn takes_only_pngs() {
        assert!(png(b"\x89PNG\r\n\x1a\nrest".to_vec()).is_some());
        assert!(png(b"PAM picture".to_vec()).is_none());
    }

    #[test]
    fn checks_options() {
        assert!(check_option("PCSX2", "-", "-fullscreen", "").is_ok());
        assert!(check_option("PCSX2", "-", "--fullscreen", "").is_err());
        assert!(check_option("RPCS3", "--", "--fullscreen", "").is_ok());
        assert!(check_option("RPCS3", "--", "fullscreen", "").is_err());
        assert!(check_option("ScummVM", "--", "--gfx-mode", "open gl").is_err());
    }

    #[test]
    fn names_images() {
        assert_eq!(image_name(Path::new("/g/Crazy Taxi.cdi")).as_deref(), Some("Crazy Taxi"));
        assert_eq!(image_name(Path::new("/g/Shenmue/disc.gdi")).as_deref(), Some("Shenmue"));
    }
}
