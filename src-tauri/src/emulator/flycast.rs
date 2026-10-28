//! Flycast, the Dreamcast emulator.

use super::{is_word, Emulator, Fix, Media};
use crate::external::{ExternalGame, Kind};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

const FLATPAK: &str = "org.flycast.Flycast";

/// Dreamcast disc images Flycast opens.
const IMAGES: &[&str] = &["gdi", "cdi", "chd", "cue"];

pub struct Flycast;

/// How Flycast is installed.
enum Install {
    Native(PathBuf),
    /// From Flathub, which can read every file (`--filesystem=host:ro`).
    Flatpak,
}

impl Install {
    /// `flycast` on `$PATH`, else the Flatpak, per user or system-wide.
    fn find() -> Option<Install> {
        let path = std::env::var_os("PATH").unwrap_or_default();
        if let Some(bin) = std::env::split_paths(&path).map(|dir| dir.join("flycast")).find(|p| p.is_file()) {
            return Some(Install::Native(bin));
        }
        let user = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/flatpak"));
        user.into_iter()
            .chain([PathBuf::from("/var/lib/flatpak")])
            .any(|root| root.join("app").join(FLATPAK).join("current").exists())
            .then_some(Install::Flatpak)
    }

    /// Opens `rom` with `settings` as `-config` options.
    fn command(&self, rom: &Path, settings: &BTreeMap<String, String>) -> Command {
        let mut cmd = match self {
            Install::Native(bin) => Command::new(bin),
            Install::Flatpak => {
                let mut cmd = Command::new("flatpak");
                cmd.arg("run").arg(FLATPAK);
                cmd
            }
        };
        for (key, value) in settings {
            cmd.arg("-config").arg(format!("{key}={value}"));
        }
        // Last, where `running_marker` looks for it.
        cmd.arg(rom);
        cmd
    }
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

    /// From Flathub, the same everywhere.
    fn install_hint(&self) -> Option<String> {
        Some(format!("flatpak install flathub {FLATPAK}"))
    }

    fn installed(&self) -> bool {
        Install::find().is_some()
    }

    fn media(&self) -> Media {
        Media::Image { extensions: IMAGES.iter().map(|e| e.to_string()).collect() }
    }

    /// Settings go in as `-config section:key=value`, for this run only: they
    /// don't touch `emu.cfg`.
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
        let rom = game.rom.as_ref().ok_or("Falta la imagen del disco")?;
        Ok(Install::find().ok_or_else(|| self.missing())?.command(rom, settings))
    }

    fn running_marker(&self, game: &ExternalGame) -> String {
        let rom = game.rom.as_deref().unwrap_or(&game.path);
        format!("{} ", rom.to_string_lossy().to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_settings_before_the_image() {
        let args = |cmd: Command| cmd.as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>();
        let rom = Path::new("/g/Sonic/disc.gdi");
        assert_eq!(
            args(Install::Flatpak.command(rom, &Flycast.default_settings())),
            ["run", FLATPAK, "-config", "rend:DupeFrames=yes", "/g/Sonic/disc.gdi"]
        );
        assert_eq!(args(Install::Native("/usr/bin/flycast".into()).command(rom, &BTreeMap::new())), ["/g/Sonic/disc.gdi"]);
    }

    #[test]
    fn validates_settings() {
        assert!(Flycast.check_setting("aica:Volf", "0.5").is_ok());
        assert!(Flycast.check_setting("DupeFrames", "yes").is_err());
        assert!(Flycast.check_setting("rend:", "yes").is_err());
        assert!(Flycast.check_setting("rend:vsync", "no,rend:Fog=no").is_err());
    }
}
