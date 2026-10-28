//! Emulators: the programs that run games of other systems (Dreamcast) or
//! engines (ScummVM) natively, without Wine. Each says how it's installed,
//! what it opens, how it starts a game and which of its settings fix known
//! problems, so the UI shows each one's own options without knowing it.
//!
//! Adding one takes a `Kind` variant, a module implementing `Emulator`, and
//! its entry in `ALL`.

mod flycast;
mod scummvm;

use crate::external::{ExternalGame, Kind};
use serde::Serialize;
use std::collections::BTreeMap;
use tokio::process::Command;
use ts_rs::TS;

pub use scummvm::detect as detect_scummvm;

/// Every emulator, in the order the sidebar lists their systems.
static ALL: &[&dyn Emulator] = &[&scummvm::Scummvm, &flycast::Flycast];

/// What the player picks when adding a game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, rename = "EmulatorMedia")]
pub enum Media {
    /// A disc image, opened from where it is.
    Image { extensions: Vec<String> },
    /// The game's ISO images, extracted into the app's folder, or its
    /// folder, used as is; the emulator tells which game it is.
    GameFiles,
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
    pub media: Media,
    pub fixes: Vec<Fix>,
}

pub trait Emulator: Sync {
    fn kind(&self) -> Kind;
    fn system(&self) -> &'static str;
    fn program(&self) -> &'static str;
    /// The command that installs it on this system, when known.
    fn install_hint(&self) -> Option<String>;
    fn installed(&self) -> bool;
    fn media(&self) -> Media;
    fn fixes(&self) -> Vec<Fix>;
    /// Rejects a setting the emulator couldn't take on its command line.
    fn check_setting(&self, key: &str, value: &str) -> Result<(), String>;
    /// The command that runs `game` with `settings`.
    fn command(&self, game: &ExternalGame, settings: &BTreeMap<String, String>) -> Result<Command, String>;
    /// What the command line of the running game contains. Command lines end
    /// in a space, standing for each argument's NUL, and so must this, so
    /// that `/g/Alive` doesn't match `/g/Alive2`.
    fn running_marker(&self, game: &ExternalGame) -> String;

    /// Why it can't run anything: it isn't installed.
    fn missing(&self) -> String {
        match self.install_hint() {
            Some(command) => format!("{} no está instalado ({command})", self.program()),
            None => format!("{} no está instalado", self.program()),
        }
    }

    fn require(&self) -> Result<(), String> {
        if self.installed() { Ok(()) } else { Err(self.missing()) }
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
            media: self.media(),
            fixes: self.fixes(),
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

/// A setting part made only of letters, digits and `extra`.
fn is_word(s: &str, extra: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_but_windows_has_one_emulator() {
        for kind in [Kind::Scummvm, Kind::Dreamcast] {
            assert_eq!(ALL.iter().filter(|e| e.kind() == kind).count(), 1, "{kind:?}");
        }
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
}
