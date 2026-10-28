//! Flycast, the Dreamcast emulator.

use super::program::{Install, Program};
use super::{is_word, rom, Emulator, Fix, Media};
use crate::external::{ExternalGame, Kind};
use std::collections::BTreeMap;
use std::path::Path;
use tokio::process::Command;

const PROGRAM: Program = Program { bins: &["flycast"], paths: &[], flatpak: Some("org.flycast.Flycast") };

pub struct Flycast;

/// Opens `rom`, in `dir`, with `settings` as `-config` options, which apply
/// to this run only: they don't touch `emu.cfg`.
fn command(install: &Install, dir: &Path, rom: &Path, settings: &BTreeMap<String, String>) -> Command {
    let mut cmd = install.command(&[dir]);
    for (key, value) in settings {
        cmd.arg("-config").arg(format!("{key}={value}"));
    }
    // Last, where `running_marker` looks for it.
    cmd.arg(rom);
    cmd
}

impl Emulator for Flycast {
    fn kind(&self) -> Kind {
        Kind::Dreamcast
    }

    fn system(&self) -> &'static str {
        "Dreamcast"
    }

    fn program(&self) -> &'static str {
        "Flycast"
    }

    fn find(&self) -> Option<Install> {
        PROGRAM.find()
    }

    fn install_hint(&self) -> Option<String> {
        PROGRAM.flatpak_hint()
    }

    fn media(&self) -> Media {
        Media::image(self.program(), &["gdi", "cdi", "chd", "cue"])
    }

    fn fixes(&self) -> Vec<Fix> {
        vec![Fix {
            symptom: "El juego va demasiado rápido".into(),
            // With VSync on, games went 4× too fast on a 240 Hz monitor.
            explanation: "Con la sincronización vertical, Flycast avanza un fotograma por cada refresco de la \
                pantalla, así que en monitores de 120 Hz o más el juego corre a 2×, 4×… Repite cada fotograma \
                para mantener los 60 Hz de la Dreamcast. En pantallas de 60 Hz no hace nada."
                .into(),
            key: "rend:DupeFrames".into(),
            value: "yes".into(),
            default: true,
        }]
    }

    /// Flycast reads `-config` as `section:key=value,...`, so neither part may
    /// hold a `,`, and the key needs its section.
    fn check_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let valid_key = key.split_once(':').is_some_and(|(sect, k)| is_word(sect, "._") && is_word(k, "._"));
        if valid_key && !value.contains(',') {
            Ok(())
        } else {
            Err(format!("Ajuste de Flycast no válido: «{key}={value}»"))
        }
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
        let (dir, rom) = (Path::new("/g/Sonic"), Path::new("/g/Sonic/disc.gdi"));
        let flatpak = Install::Flatpak("org.flycast.Flycast");
        assert_eq!(
            args(&command(&flatpak, dir, rom, &Flycast.default_settings())),
            ["run", "--filesystem=/g/Sonic:ro", "org.flycast.Flycast", "-config", "rend:DupeFrames=yes", "/g/Sonic/disc.gdi"]
        );
        assert_eq!(args(&command(&Install::Native("/usr/bin/flycast".into()), dir, rom, &BTreeMap::new())), ["/g/Sonic/disc.gdi"]);
    }

    #[test]
    fn validates_settings() {
        assert!(Flycast.check_setting("aica:Volf", "0.5").is_ok());
        assert!(Flycast.check_setting("DupeFrames", "yes").is_err());
        assert!(Flycast.check_setting("rend:", "yes").is_err());
        assert!(Flycast.check_setting("rend:vsync", "no,rend:Fog=no").is_err());
    }
}
