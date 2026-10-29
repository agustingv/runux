//! PCSX2, the PlayStation 2 emulator.

use super::program::{Install, Program};
use crate::help::tilde;
use super::{check_option, duckstation, list, playstation, rom, section, Cover, Emulator, Fix, HelpSection, Media};
use crate::external::{ExternalGame, Kind};
use std::collections::BTreeMap;
use std::path::Path;
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

    fn cover(&self, rom: &Path) -> Option<Cover> {
        let db = PROGRAM.find().and_then(|i| playstation::read_db(&i, "GameIndex.yaml"));
        Some(Cover::xlenore("ps2-covers", "ps2", &playstation::serial(rom, db.as_deref())?))
    }

    fn media(&self) -> Media {
        Media::image(self.program(), &["iso", "chd", "cso", "zso", "gz", "bin"])
    }

    fn fixes(&self) -> Vec<Fix> {
        duckstation::fixes(self.program(), "PlayStation 2")
    }

    fn help(&self) -> Vec<HelpSection> {
        let install = PROGRAM.find_or_flatpak();
        let config = install.as_ref().and_then(|i| i.config_dir("PCSX2"));
        let memcards = config.as_ref().map(|c| tilde(&c.join("memcards"))).unwrap_or_else(|| "memcards".into());
        vec![
            duckstation::bios_help(
                install.as_ref(),
                self.program(),
                "PlayStation 2",
                "La web de PCSX2 explica cómo volcarla con un programa que se ejecuta en la propia consola.",
                config.map(|c| c.join("bios")),
            ),
            section(
                "Qué imagen añadir",
                vec![list([
                    "Una `.iso` del DVD. Runux no lee el disco del lector: haz antes una imagen de él.".to_string(),
                    "Un `.chd`, `.cso` o `.zso`: el disco comprimido, que ocupa bastante menos.".into(),
                    "Un `.bin` de los pocos juegos que venían en CD.".into(),
                ])],
            ),
            duckstation::pad_help(self.program()),
            duckstation::cover_help("ps2-covers"),
            section(
                "Si algo falla",
                vec![list([
                    "Si va lento o se ve mal, prueba en los ajustes de gráficos de PCSX2 otro renderizador (Vulkan u \
                     OpenGL) o menos resolución.".to_string(),
                    format!("Las tarjetas de memoria las crea PCSX2 solo; las partidas guardadas están en `{memcards}`."),
                    "Los gráficos y el resto de ajustes se configuran en PCSX2, y valen para todos sus juegos.".into(),
                ])],
            ),
        ]
    }

    fn check_setting(&self, key: &str, value: &str) -> Result<(), String> {
        check_option(self.program(), "-", key, value)
    }

    fn command(&self, game: &ExternalGame, settings: &BTreeMap<String, String>) -> Result<Command, String> {
        Ok(duckstation::command(&self.require()?, &game.path, rom(game)?, settings))
    }
}
