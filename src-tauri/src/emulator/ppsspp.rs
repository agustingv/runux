//! PPSSPP, the PSP emulator.

use super::program::{Install, Program};
use super::{is_iso, iso_art, list, option_args, png, Art, MAX_ART, rom, section, text, Emulator, Fix, HelpSection, Media};
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

/// The file `index` of a PlayStation Store `EBOOT.PBP`: a header with the
/// offsets of `PARAM.SFO`, `ICON0.PNG`, `ICON1.PMF`, `PIC0.PNG`, `PIC1.PNG`,
/// `SND0.AT3`, `DATA.PSP` and `DATA.PSAR`, each ending where the next starts.
fn pbp_file(pbp: &Path, index: usize) -> Option<Vec<u8>> {
    use std::os::unix::fs::FileExt;
    let file = std::fs::File::open(pbp).ok()?;
    let mut header = [0; 40];
    file.read_exact_at(&mut header, 0).ok()?;
    if &header[..4] != b"\0PBP" {
        return None;
    }
    let offset = |i: usize| Some(u32::from_le_bytes(header.get(8 + i * 4..12 + i * 4)?.try_into().ok()?));
    let (start, end) = (offset(index)?, offset(index + 1)?);
    let len = end.checked_sub(start).filter(|&len| len <= MAX_ART)?;
    let mut data = vec![0; len as usize];
    file.read_exact_at(&mut data, start as u64).ok()?;
    Some(data)
}

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

    /// `ICON0.PNG` and `PIC1.PNG`, in `PSP_GAME/` on a disc, packed in a
    /// PlayStation Store game's `.pbp`.
    fn art(&self, rom: &Path, which: Art) -> Option<Vec<u8>> {
        let (name, index) = match which {
            Art::Icon => ("ICON0.PNG", 1),
            Art::Background => ("PIC1.PNG", 4),
        };
        if is_iso(rom) {
            iso_art(rom, &format!("PSP_GAME/{name}"))
        } else if rom.extension().is_some_and(|e| e.eq_ignore_ascii_case("pbp")) {
            png(pbp_file(rom, index)?)
        } else {
            None
        }
    }

    fn media(&self) -> Media {
        Media::image(self.program(), &["iso", "cso", "chd", "pbp"])
    }

    fn help(&self) -> Vec<HelpSection> {
        vec![
            section(
                "Antes de jugar",
                vec![text("PPSSPP no necesita BIOS ni firmware: basta con añadir el juego.")],
            ),
            section(
                "Qué imagen añadir",
                vec![list([
                    "Una `.iso` del UMD, o un `.cso` o `.chd`, que son la imagen comprimida.",
                    "Un `.pbp`, como vienen los juegos de la PlayStation Store.",
                ])],
            ),
            section(
                "Mandos",
                vec![text(
                    "PPSSPP reconoce la mayoría de mandos, como los de PlayStation o Xbox, sin configurar nada. Si \
                     alguno no responde o quieres cambiar los botones, ve en PPSSPP a Ajustes → Controles → \
                     Asignación de controles.",
                )],
            ),
            section(
                "Si algo falla",
                vec![list([
                    "Esc abre el menú de PPSSPP, para guardar la partida o salir.",
                    "Los gráficos y el resto de ajustes se configuran en PPSSPP, y valen para todos sus juegos.",
                ])],
            ),
        ]
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
    fn reads_pictures_from_pbps() {
        // Offsets: an empty PARAM.SFO, ICON0, no ICON1 or PIC0, PIC1, the rest empty.
        let icon = b"\x89PNG\r\n\x1a\nicon";
        let pic = b"\x89PNG\r\n\x1a\npic1";
        let at = |n: usize| (40 + n as u32).to_le_bytes();
        let offsets = [at(0), at(0), at(12), at(12), at(12), at(24), at(24), at(24)];
        let mut pbp = b"\0PBP\0\0\x01\0".to_vec();
        pbp.extend(offsets.concat());
        pbp.extend(icon);
        pbp.extend(pic);
        let path = std::env::temp_dir().join(format!("runux-pbp-{}.pbp", std::process::id()));
        std::fs::write(&path, &pbp).unwrap();
        assert_eq!(Ppsspp.art(&path, Art::Icon).as_deref(), Some(&icon[..]));
        assert_eq!(Ppsspp.art(&path, Art::Background).as_deref(), Some(&pic[..]));
        std::fs::write(&path, b"not a pbp, not at all; just some text").unwrap();
        assert_eq!(Ppsspp.art(&path, Art::Icon), None);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn passes_settings_before_the_image() {
        let settings = BTreeMap::from([("--fullscreen".to_string(), String::new())]);
        let cmd = command(&Install::Native("/usr/bin/PPSSPPSDL".into()), Path::new("/g"), Path::new("/g/Patapon.iso"), &settings);
        assert_eq!(args(&cmd), ["--fullscreen", "/g/Patapon.iso"]);
    }
}
