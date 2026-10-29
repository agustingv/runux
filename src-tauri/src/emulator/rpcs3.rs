//! RPCS3, the PlayStation 3 emulator.

use super::program::{Install, Program};
use crate::help::tilde;
use super::{file_art, image_name, is_iso, iso_art, list, Art, option_args, rom, section, steps, text, Emulator, Fix, HelpSection, Media};
use crate::external::{ExternalGame, Kind};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

const PROGRAM: Program = Program { bins: &["rpcs3"], paths: &[], flatpak: Some("net.rpcs3.RPCS3") };

pub struct Rpcs3;

/// Opens `rom`, in `dir`, without RPCS3's game list, so it exits with the
/// game, and with `settings` as `--options`.
fn command(install: &Install, dir: &Path, rom: &Path, settings: &BTreeMap<String, String>) -> Command {
    let mut cmd = install.command(&[dir]);
    cmd.arg("--no-gui").args(option_args(settings)).arg(rom);
    cmd
}

/// Where `vfs.yml` puts `/dev_flash/`, the PS3's system storage, which
/// holds the firmware: in the settings folder `config` unless moved.
fn dev_flash(config: &Path, vfs: Option<&str>) -> PathBuf {
    vfs.and_then(|vfs| vfs.lines().find_map(|l| l.trim().strip_prefix("/dev_flash/:")))
        .map(|dir| dir.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir.replace("$(EmulatorDir)", &format!("{}/", config.display()))))
        .unwrap_or_else(|| config.join("dev_flash"))
}

/// Without the firmware, RPCS3 can't load the system fonts and, with
/// `--no-gui`, hangs without a window: say so before launching it.
fn check_firmware(install: &Install) -> Result<(), String> {
    let Some(config) = install.config_dir("rpcs3") else {
        return Ok(());
    };
    let vfs = std::fs::read_to_string(config.join("vfs.yml")).ok();
    if dev_flash(&config, vfs.as_deref()).join("vsh/etc/version.txt").is_file() {
        return Ok(());
    }
    Err("Falta el firmware de la PlayStation 3, sin el que RPCS3 se queda bloqueado. Descarga PS3UPDAT.PUP de la \
         web de PlayStation, pulsa «Abrir RPCS3» e instálalo desde File → Install Firmware."
        .into())
}

/// The folder holding a whole game, from its `EBOOT.BIN`: a disc game's is
/// in `<game>/PS3_GAME/USRDIR/`, a PSN game's in `<game>/USRDIR/`.
fn game_root(rom: &Path) -> PathBuf {
    let is = |p: &Path, name: &str| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name));
    let mut dir = if rom.is_dir() { rom } else { rom.parent().unwrap_or(rom) };
    if is(rom, "EBOOT.BIN") {
        for name in ["USRDIR", "PS3_GAME"] {
            if is(dir, name) {
                dir = dir.parent().unwrap_or(dir);
            }
        }
    }
    dir.to_path_buf()
}

/// The `TITLE` in a `PARAM.SFO`: a header, a table of entries, then their
/// keys and values.
fn sfo_title(data: &[u8]) -> Option<String> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?) as usize);
    let u32_at = |at: usize| Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize);
    if data.get(..4)? != b"\0PSF" {
        return None;
    }
    let (keys, values, count) = (u32_at(8)?, u32_at(12)?, u32_at(16)?);
    // Up to `len` bytes at `at`, ending at the first NUL; a value cut short
    // by the end of the file isn't one.
    let c_str = |at: usize, len: usize| {
        let bytes = data.get(at..)?;
        let bytes = if len == usize::MAX { bytes } else { bytes.get(..len)? };
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    };
    (0..count).find_map(|i| {
        let entry = 20 + i * 16;
        let key = c_str(keys + u16_at(entry)?, usize::MAX)?;
        (key == "TITLE").then(|| c_str(values + u32_at(entry + 12)?, u32_at(entry + 4)?))?
    })
}

impl Emulator for Rpcs3 {
    fn kind(&self) -> Kind {
        Kind::Playstation3
    }

    fn system(&self) -> &'static str {
        "PlayStation 3"
    }

    fn program(&self) -> &'static str {
        "RPCS3"
    }

    fn find(&self) -> Option<Install> {
        PROGRAM.find()
    }

    fn install_hint(&self) -> Option<String> {
        PROGRAM.flatpak_hint()
    }

    fn setup(&self) -> Option<String> {
        Some(
            "Necesita el firmware de la PlayStation 3 (PS3UPDAT.PUP, de la web de PlayStation). Abre RPCS3 e \
             instálalo desde File → Install Firmware."
                .into(),
        )
    }

    fn media(&self) -> Media {
        Media::File {
            label: "Juego".into(),
            help: "La carpeta del juego (la que contiene PS3_GAME o USRDIR), su EBOOT.BIN o una imagen .iso. \
                Se abre con RPCS3 desde donde está; no se copia."
                .into(),
            extensions: vec!["iso".into(), "bin".into()],
            folder: true,
        }
    }

    fn fixes(&self) -> Vec<Fix> {
        vec![Fix::flag("--fullscreen", "La ventana del juego es muy pequeña", "Abre el juego a pantalla completa.")]
    }

    fn help(&self) -> Vec<HelpSection> {
        let install = PROGRAM.find_or_flatpak();
        let at = |dir: Option<PathBuf>, file: &str| dir.map(|d| tilde(&d.join(file))).unwrap_or_else(|| file.into());
        let games = at(install.as_ref().and_then(|i| i.config_dir("rpcs3")), "dev_hdd0/game");
        let log = at(install.as_ref().and_then(|i| i.cache_dir("rpcs3")), "RPCS3.log");
        vec![
            section(
                "Antes de jugar: el firmware",
                vec![
                    text(
                        "RPCS3 necesita el firmware de la PlayStation 3, el sistema de la consola, que Sony ofrece \
                         gratis. Sin él los juegos se quedan bloqueados; Runux te avisa si falta.",
                    ),
                    steps([
                        "Descarga `PS3UPDAT.PUP` de la web de PlayStation: busca «actualización del software del \
                         sistema PS3».",
                        "Pulsa «Abrir RPCS3» en la página de un juego y, en su menú, File → Install Firmware. Elige \
                         el archivo y espera a que termine.",
                    ]),
                ],
            ),
            section(
                "Qué añadir",
                vec![list([
                    "La carpeta de un juego de disco: la que contiene `PS3_GAME`.".to_string(),
                    "Su `EBOOT.BIN`, si es más cómodo: Runux sube hasta la carpeta del juego.".into(),
                    "Una `.iso` del disco. Si RPCS3 no la abre, extrae su contenido a una carpeta y añade esa.".into(),
                    format!(
                        "Un juego de la PlayStation Store (`.pkg`): instálalo antes en RPCS3, con File → Install \
                         Packages/Raps/Edats junto a su licencia `.rap`, y añade su carpeta, en `{games}`."
                    ),
                ])],
            ),
            section(
                "La primera vez",
                vec![text(
                    "La primera vez que abres un juego, RPCS3 compila su código para tu procesador y puede tardar \
                     varios minutos en mostrar nada. Las siguientes veces arranca mucho antes.",
                )],
            ),
            section(
                "Mandos",
                vec![
                    text(
                        "RPCS3 empieza con el jugador 1 en el teclado. Asígnale el mando una vez y valdrá para todos \
                         los juegos, también los que lances desde Runux.",
                    ),
                    steps([
                        "Pulsa «Abrir RPCS3» en la página de un juego y luego el botón «Pads» de su barra.",
                        "En Player 1, cambia Handler de `Keyboard` a `DualShock 4` (o `DualSense`, para uno de PS5) \
                         y elige el mando en Device.",
                        "Pulsa «Save».",
                    ]),
                    text(
                        "Si el mando no aparece con ese handler, elige `SDL`, que reconoce casi cualquier mando. Para \
                         uno de Xbox, `Evdev` o `SDL`.",
                    ),
                ],
            ),
            section(
                "Si algo falla",
                vec![list([
                    format!("RPCS3 apunta lo que pasa en `{log}`, que se reescribe cada vez que lo abres."),
                    "Avisos como «Failed to lock sudo memory» piden subir el límite de memoria bloqueada del sistema \
                     (`memlock`). Suelen afectar solo un poco al rendimiento."
                        .into(),
                    "No todos los juegos funcionan igual de bien: su web tiene una lista de compatibilidad por juego."
                        .into(),
                    "Los gráficos y el resto de ajustes se configuran en RPCS3, y valen para todos sus juegos."
                        .into(),
                ])],
            ),
        ]
    }

    fn command(&self, game: &ExternalGame, settings: &BTreeMap<String, String>) -> Result<Command, String> {
        let install = self.require()?;
        check_firmware(&install)?;
        Ok(command(&install, &game.path, rom(game)?, settings))
    }

    fn game_dir(&self, rom: &Path) -> PathBuf {
        game_root(rom)
    }

    /// `ICON0.PNG` and `PIC1.PNG`, in `PS3_GAME/` on a disc, at the top of a
    /// PlayStation Store game's folder.
    fn art(&self, rom: &Path, which: Art) -> Option<Vec<u8>> {
        let name = match which {
            Art::Icon => "ICON0.PNG",
            Art::Background => "PIC1.PNG",
        };
        if is_iso(rom) {
            return iso_art(rom, &format!("PS3_GAME/{name}"));
        }
        let root = game_root(rom);
        [root.join("PS3_GAME").join(name), root.join(name)].iter().find_map(|p| file_art(p))
    }

    /// The title in the game's `PARAM.SFO`, else the folder's or image's name.
    fn game_name(&self, rom: &Path) -> Option<String> {
        if is_iso(rom) {
            return image_name(rom);
        }
        let root = game_root(rom);
        [root.join("PS3_GAME/PARAM.SFO"), root.join("PARAM.SFO")]
            .iter()
            .find_map(|sfo| sfo_title(&std::fs::read(sfo).ok()?))
            .filter(|t| !t.trim().is_empty())
            .or_else(|| image_name(&root))
    }
}

#[cfg(test)]
mod tests {
    use super::super::program::args;
    use super::*;

    /// A `PARAM.SFO` holding `entries`, all UTF-8 strings.
    fn sfo(entries: &[(&str, &str)]) -> Vec<u8> {
        let (mut keys, mut values, mut table) = (Vec::new(), Vec::new(), Vec::new());
        for (k, v) in entries {
            table.extend((keys.len() as u16).to_le_bytes());
            table.extend(0x0204u16.to_le_bytes());
            table.extend((v.len() as u32 + 1).to_le_bytes());
            table.extend((v.len() as u32 + 1).to_le_bytes());
            table.extend((values.len() as u32).to_le_bytes());
            keys.extend(k.bytes().chain([0]));
            values.extend(v.bytes().chain([0]));
        }
        let keys_at = 20 + table.len();
        let values_at = keys_at + keys.len();
        let mut data = b"\0PSF".to_vec();
        data.extend(0x101u32.to_le_bytes());
        data.extend((keys_at as u32).to_le_bytes());
        data.extend((values_at as u32).to_le_bytes());
        data.extend((entries.len() as u32).to_le_bytes());
        data.extend(table);
        data.extend(keys);
        data.extend(values);
        data
    }

    #[test]
    fn reads_the_title() {
        assert_eq!(sfo_title(&sfo(&[("CATEGORY", "DG"), ("TITLE", "Demon's Souls™")])).as_deref(), Some("Demon's Souls™"));
        assert_eq!(sfo_title(&sfo(&[("CATEGORY", "DG")])), None);
        assert_eq!(sfo_title(b"not an sfo"), None);
        let mut cut = sfo(&[("TITLE", "Journey")]);
        cut.truncate(cut.len() - 4);
        assert_eq!(sfo_title(&cut), None);
    }

    #[test]
    fn finds_the_whole_game_from_its_eboot() {
        assert_eq!(game_root(Path::new("/g/DeS/PS3_GAME/USRDIR/EBOOT.BIN")), Path::new("/g/DeS"));
        assert_eq!(game_root(Path::new("/g/Journey/USRDIR/EBOOT.BIN")), Path::new("/g/Journey"));
        assert_eq!(game_root(Path::new("/g/Uncharted.iso")), Path::new("/g"));
    }

    #[test]
    fn finds_the_firmware_where_vfs_puts_it() {
        let config = Path::new("/c/rpcs3");
        assert_eq!(dev_flash(config, None), Path::new("/c/rpcs3/dev_flash"));
        let vfs = "$(EmulatorDir): \"\"\n/dev_flash/: $(EmulatorDir)dev_flash/\n/dev_hdd0/: /big/hdd0/\n";
        assert_eq!(dev_flash(config, Some(vfs)), Path::new("/c/rpcs3/dev_flash/"));
        assert_eq!(dev_flash(config, Some("/dev_flash/: /ps3/flash/\n")), Path::new("/ps3/flash/"));
    }

    #[test]
    fn runs_without_the_game_list() {
        let settings = BTreeMap::from([("--fullscreen".to_string(), String::new())]);
        let cmd = command(&Install::Flatpak("net.rpcs3.RPCS3"), Path::new("/g/DeS"), Path::new("/g/DeS"), &settings);
        assert_eq!(args(&cmd), ["run", "--filesystem=/g/DeS:ro", "net.rpcs3.RPCS3", "--no-gui", "--fullscreen", "/g/DeS"]);
    }
}
