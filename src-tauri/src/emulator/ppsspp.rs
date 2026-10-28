//! PPSSPP, the PSP emulator.

use super::program::{Install, Program};
use super::{option_args, rom, Emulator, Fix, Media};
use crate::external::{ExternalGame, Kind};
use std::collections::BTreeMap;
use std::path::Path;
use tokio::process::Command;

const PROGRAM: Program = Program {
    bins: &["PPSSPPSDL", "PPSSPPQt", "ppsspp"],
    paths: &[],
    flatpak: Some("org.ppsspp.PPSSPP"),
};

pub struct Ppsspp;

/// Opens `rom`, in `dir`, with `settings` as `--options`.
fn command(install: &Install, dir: &Path, rom: &Path, settings: &BTreeMap<String, String>) -> Command {
    let mut cmd = install.command(&[dir]);
    cmd.args(option_args(settings)).arg(rom);
    cmd
}

impl Emulator for Ppsspp {
    fn kind(&self) -> Kind {
        Kind::Psp
    }

    fn system(&self) -> &'static str {
        "PSP"
    }

    fn program(&self) -> &'static str {
        "PPSSPP"
    }

    fn find(&self) -> Option<Install> {
        PROGRAM.find()
    }

    fn install_hint(&self) -> Option<String> {
        PROGRAM.flatpak_hint()
    }

    fn media(&self) -> Media {
        Media::image(self.program(), &["iso", "cso", "chd", "pbp"])
    }

    fn fixes(&self) -> Vec<Fix> {
        vec![Fix::flag(
            "--fullscreen",
            "La ventana del juego es muy pequeña",
            "Abre el juego a pantalla completa, aunque PPSSPP tenga guardado otro modo.",
        )]
    }

    fn command(&self, game: &ExternalGame, settings: &BTreeMap<String, String>) -> Result<Command, String> {
        Ok(command(&self.require()?, &game.path, rom(game)?, settings))
    }
}

#[cfg(test)]
mod tests {
    use super::super::program::args;
    use super::*;

    #[test]
    fn passes_settings_before_the_image() {
        let settings = BTreeMap::from([("--fullscreen".to_string(), String::new())]);
        let cmd = command(&Install::Native("/usr/bin/PPSSPPSDL".into()), Path::new("/g"), Path::new("/g/Patapon.iso"), &settings);
        assert_eq!(args(&cmd), ["--fullscreen", "/g/Patapon.iso"]);
    }
}
