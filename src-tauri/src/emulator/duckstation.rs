//! DuckStation, the PlayStation emulator.

use super::program::{Install, Program};
use super::{check_option, option_args, rom, Emulator, Fix, Media};
use crate::external::{ExternalGame, Kind};
use std::collections::BTreeMap;
use std::path::Path;
use tokio::process::Command;

/// Its author recommends the AppImage, which may be on `$PATH` under either
/// name; the Flatpak is no longer updated, but works.
const PROGRAM: Program = Program {
    bins: &["duckstation-qt", "duckstation"],
    paths: &[],
    flatpak: Some("org.duckstation.DuckStation"),
};

pub struct Duckstation;

/// Opens `rom`, in `dir`, with `settings` as `-flags`, exiting when the game
/// powers off (`-batch`). PCSX2 takes the same command line.
pub(super) fn command(install: &Install, dir: &Path, rom: &Path, settings: &BTreeMap<String, String>) -> Command {
    let mut cmd = install.command(&[dir]);
    cmd.arg("-batch").args(option_args(settings));
    // `--` keeps names starting with `-` from being read as options.
    cmd.arg("--").arg(rom);
    cmd
}

/// Its own fixes, which PCSX2 shares.
pub(super) fn fixes(program: &str, bios: &str) -> Vec<Fix> {
    vec![
        Fix::flag(
            "-fullscreen",
            "La ventana del juego es muy pequeña",
            &format!("Abre el juego a pantalla completa, aunque {program} tenga guardado otro modo."),
        ),
        Fix::flag(
            "-fastboot",
            "Tarda en empezar por la animación de arranque",
            &format!("Se salta la animación de la BIOS de la {bios} y va directo al juego. Algún juego puede fallar sin ella."),
        ),
    ]
}

impl Emulator for Duckstation {
    fn kind(&self) -> Kind {
        Kind::Playstation
    }

    fn system(&self) -> &'static str {
        "PlayStation"
    }

    fn program(&self) -> &'static str {
        "DuckStation"
    }

    fn find(&self) -> Option<Install> {
        PROGRAM.find()
    }

    fn install_hint(&self) -> Option<String> {
        PROGRAM.flatpak_hint()
    }

    fn setup(&self) -> Option<String> {
        Some(
            "Necesita la BIOS de una PlayStation, volcada de tu consola. Abre DuckStation: su asistente inicial \
             te pide la carpeta donde está."
                .into(),
        )
    }

    fn media(&self) -> Media {
        Media::image(self.program(), &["cue", "chd", "pbp", "m3u", "bin", "img", "iso", "ecm", "mds", "ccd"])
    }

    fn fixes(&self) -> Vec<Fix> {
        fixes(self.program(), "PlayStation")
    }

    fn check_setting(&self, key: &str, value: &str) -> Result<(), String> {
        check_option(self.program(), "-", key, value)
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
    fn exits_with_the_game_and_ends_options() {
        let settings = BTreeMap::from([("-fullscreen".to_string(), String::new())]);
        let flatpak = Install::Flatpak("org.duckstation.DuckStation");
        let cmd = command(&flatpak, Path::new("/g/FF7"), Path::new("/g/FF7/-Disc 1.cue"), &settings);
        assert_eq!(
            args(&cmd),
            ["run", "--filesystem=/g/FF7:ro", "org.duckstation.DuckStation", "-batch", "-fullscreen", "--", "/g/FF7/-Disc 1.cue"]
        );
    }
}
