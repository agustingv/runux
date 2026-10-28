//! PCSX2, the PlayStation 2 emulator.

use super::program::{Install, Program};
use super::{check_option, duckstation, rom, Emulator, Fix, Media};
use crate::external::{ExternalGame, Kind};
use std::collections::BTreeMap;
use tokio::process::Command;

const PROGRAM: Program = Program { bins: &["pcsx2-qt", "pcsx2"], paths: &[], flatpak: Some("net.pcsx2.PCSX2") };

pub struct Pcsx2;

impl Emulator for Pcsx2 {
    fn kind(&self) -> Kind {
        Kind::Playstation2
    }

    fn system(&self) -> &'static str {
        "PlayStation 2"
    }

    fn program(&self) -> &'static str {
        "PCSX2"
    }

    fn find(&self) -> Option<Install> {
        PROGRAM.find()
    }

    fn install_hint(&self) -> Option<String> {
        PROGRAM.flatpak_hint()
    }

    fn setup(&self) -> Option<String> {
        Some(
            "Necesita la BIOS de una PlayStation 2, volcada de tu consola. Abre PCSX2: su asistente inicial te \
             pide la carpeta donde está."
                .into(),
        )
    }

    fn media(&self) -> Media {
        Media::image(self.program(), &["iso", "chd", "cso", "zso", "gz", "bin"])
    }

    fn fixes(&self) -> Vec<Fix> {
        duckstation::fixes(self.program(), "PlayStation 2")
    }

    fn check_setting(&self, key: &str, value: &str) -> Result<(), String> {
        check_option(self.program(), "-", key, value)
    }

    fn command(&self, game: &ExternalGame, settings: &BTreeMap<String, String>) -> Result<Command, String> {
        Ok(duckstation::command(&self.require()?, &game.path, rom(game)?, settings))
    }
}
