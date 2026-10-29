//! DuckStation, the PlayStation emulator.

use super::program::{Install, Program};
use crate::help::tilde;
use super::{check_option, list, option_args, rom, section, steps, text, Emulator, Fix, HelpSection, Media};
use crate::external::{ExternalGame, Kind};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
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

/// A disc with LibCrypt copy protection, which DuckStation won't start
/// without its subchannel data.
#[derive(Debug, PartialEq)]
struct Protected {
    serial: String,
    name: String,
}

/// The serials (`SLES-02529`) in a file name, which may write them
/// `sles_025.29`, as on the disc.
fn serials(name: &str) -> Vec<String> {
    let b = name.to_ascii_uppercase().into_bytes();
    let mut found = Vec::new();
    for i in 0..b.len() {
        let rest = &b[i..];
        if (i > 0 && b[i - 1].is_ascii_alphanumeric()) || rest.len() < 9 || !rest[..4].iter().all(u8::is_ascii_uppercase) {
            continue;
        }
        let mut k = if matches!(rest[4], b'-' | b'_' | b' ') { 5 } else { 4 };
        let mut digits = String::new();
        while digits.len() < 5 {
            match rest.get(k) {
                Some(c) if c.is_ascii_digit() => digits.push(*c as char),
                Some(b'.') if digits.len() == 3 => {}
                _ => break,
            }
            k += 1;
        }
        if digits.len() == 5 && !rest.get(k).is_some_and(u8::is_ascii_digit) {
            found.push(format!("{}-{digits}", String::from_utf8_lossy(&rest[..4])));
        }
    }
    found
}

/// The protected game, in DuckStation's `gamedb.yaml`, whose serial is in one
/// of `names` or whose title is one of them without its extensions.
fn libcrypt_game(gamedb: &str, names: &[String]) -> Option<Protected> {
    let wanted: Vec<String> = names.iter().flat_map(|n| serials(n)).collect();
    let titles: Vec<String> = names.iter().map(|n| n.split('.').next().unwrap_or(n).trim().to_lowercase()).collect();
    let matches = |codes: &[String], name: &str| {
        let by_title = || titles.contains(&name.to_lowercase()).then(|| codes[0].clone());
        codes.iter().find(|c| wanted.contains(c)).cloned().or_else(by_title)
    };
    // Entries are `SERIAL:` with indented fields, and more serials under `codes:`.
    let (mut codes, mut name, mut libcrypt, mut section) = (Vec::<String>::new(), String::new(), false, "");
    for line in gamedb.lines().chain(["END:"]) {
        if !line.starts_with(' ') && line.ends_with(':') {
            if libcrypt {
                if let Some(serial) = matches(&codes, &name) {
                    return Some(Protected { serial, name });
                }
            }
            (codes, name, libcrypt) = (vec![line.trim_end_matches(':').to_string()], String::new(), false);
        } else if let Some(field) = line.strip_prefix("  ").filter(|f| !f.starts_with(' ')) {
            let (key, value) = field.split_once(':').unwrap_or((field, ""));
            section = if key == "codes" { "codes" } else { "" };
            match key {
                "name" => name = value.trim().trim_matches('"').to_string(),
                "libcrypt" => libcrypt = value.trim() == "true",
                _ => {}
            }
        } else if let Some(code) = line.trim().strip_prefix("- ").filter(|_| section == "codes") {
            codes.push(code.trim().to_string());
        }
    }
    None
}

/// Where DuckStation keeps its settings: natively, without
/// `$XDG_CONFIG_HOME`, in `~/.local/share`.
fn user_dir(install: &Install) -> Option<PathBuf> {
    match install {
        Install::Native(_) if std::env::var_os("XDG_CONFIG_HOME").is_none() => {
            Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/share/duckstation"))
        }
        _ => install.config_dir("duckstation"),
    }
}

/// DuckStation won't start a LibCrypt-protected disc without its SBI file and,
/// with `-batch`, just closes: say why before launching it.
fn check_libcrypt(install: &Install, game: &ExternalGame, rom: &Path) -> Result<(), String> {
    let ext = rom.extension().unwrap_or_default().to_string_lossy().to_lowercase();
    // A playlist or a PSN image carries its own.
    if matches!(ext.as_str(), "m3u" | "pbp") || ["sbi", "lsd"].iter().any(|e| rom.with_extension(e).is_file()) {
        return Ok(());
    }
    let allowed = user_dir(install)
        .and_then(|dir| std::fs::read_to_string(dir.join("settings.ini")).ok())
        .is_some_and(|ini| ini.lines().any(|l| l.replace(' ', "") == "AllowBootingWithoutSBIFile=true"));
    let gamedb = install.files().and_then(|dir| {
        ["bin/resources/gamedb.yaml", "resources/gamedb.yaml"].iter().find_map(|f| std::fs::read_to_string(dir.join(f)).ok())
    });
    let (Some(gamedb), false) = (gamedb, allowed) else {
        return Ok(());
    };
    let names: Vec<String> = [rom.file_name(), game.path.file_name()]
        .into_iter()
        .flatten()
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    match libcrypt_game(&gamedb, &names) {
        None => Ok(()),
        Some(Protected { serial, name }) => Err(format!(
            "«{name}» ({serial}) tiene la protección anticopia LibCrypt, y DuckStation no lo arranca sin el archivo \
             SBI con los datos de subcanal del disco, que no van en la imagen. Consigue el SBI de este juego \
             ({serial}.sbi) y cópialo junto a la imagen con el nombre «{}».",
            rom.with_extension("sbi").file_name().unwrap_or_default().to_string_lossy()
        )),
    }
}

/// How to give `program`, installed as `install`, the BIOS of the `console`,
/// kept in `dir`; `which` tells which one. PCSX2 shares it.
pub(super) fn bios_help(install: Option<&Install>, program: &str, console: &str, which: &str, dir: Option<PathBuf>) -> HelpSection {
    let dir = dir.map(|d| tilde(&d)).unwrap_or_else(|| "su carpeta bios".into());
    let open = format!("Pulsa «Abrir {program}» en la página de un juego. Su asistente inicial te pide la carpeta de la BIOS: elige `{dir}`. Si ya lo pasaste, está en los ajustes de {program}, en BIOS.");
    let put = if install.is_none_or(Install::is_flatpak) {
        format!("Copia el archivo de la BIOS a `{dir}` (créala si no existe). {program} instalado desde Flathub no ve el resto de tu carpeta personal, así que tiene que estar ahí.")
    } else {
        format!("Copia el archivo de la BIOS a `{dir}`, o a cualquier otra carpeta.")
    };
    section(
        "Antes de jugar: la BIOS",
        vec![
            text(format!(
                "{program} necesita la BIOS de una {console}: el sistema de la consola, un archivo que se vuelca de una \
                 consola tuya. {which}"
            )),
            steps([put, open]),
        ],
    )
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

    fn help(&self) -> Vec<HelpSection> {
        let install = PROGRAM.find_or_flatpak();
        let bios = install.as_ref().and_then(user_dir).map(|d| d.join("bios"));
        vec![
            bios_help(
                install.as_ref(),
                self.program(),
                "PlayStation",
                "Mejor una de la misma región que tus juegos: SCPH-5502 para los europeos, SCPH-1001 para los \
                 americanos y SCPH-5500 para los japoneses. Si hay varias, DuckStation elige la que toca.",
                bios,
            ),
            section(
                "Qué imagen añadir",
                vec![list([
                    "Un `.cue` con sus `.bin`: añade el `.cue`, que describe las pistas del disco; los `.bin` tienen \
                     que estar a su lado.",
                    "Un `.chd`: el disco entero comprimido en un solo archivo. Es la mejor opción si vas a \
                     convertir tus imágenes.",
                    "Un `.bin`, `.iso`, `.img`, `.ecm`, `.mds` o `.ccd` suelto también vale, y un `.pbp` de la \
                     PlayStation Store.",
                    "Para un juego de varios discos, crea un `.m3u`: un archivo de texto con el nombre de la imagen \
                     de cada disco, uno por línea. Añade el `.m3u` y cambia de disco desde DuckStation cuando el \
                     juego lo pida.",
                ])],
            ),
            section(
                "Juegos con protección LibCrypt",
                vec![
                    text(
                        "Muchos juegos europeos, como Resident Evil 3 o Ape Escape, tienen una protección anticopia \
                         que va en los datos de subcanal del disco, que una imagen `.bin` no guarda. DuckStation no \
                         los arranca sin un archivo `.sbi` con esos datos; Runux te avisa al darle a Jugar.",
                    ),
                    steps([
                        "Consigue el `.sbi` del juego. Se nombra por el código del disco, como `SLES-02529.sbi`: \
                         está en la carátula, en el propio disco y en el nombre de muchas imágenes.",
                        "Cópialo junto a la imagen, con su mismo nombre y la extensión `.sbi`: para `Juego.cue`, \
                         `Juego.sbi`; para `Juego.bin.ecm`, `Juego.bin.sbi`.",
                    ]),
                ],
            ),
            section(
                "Si algo falla",
                vec![list([
                    "Si se cierra nada más empezar, revisa la BIOS. Para ver el mensaje de error, abre el juego \
                     desde DuckStation (con «Abrir DuckStation»), no desde Runux.",
                    "Los mandos, los gráficos y el resto de ajustes se configuran en DuckStation, y valen para todos \
                     sus juegos.",
                ])],
            ),
        ]
    }

    fn check_setting(&self, key: &str, value: &str) -> Result<(), String> {
        check_option(self.program(), "-", key, value)
    }

    fn command(&self, game: &ExternalGame, settings: &BTreeMap<String, String>) -> Result<Command, String> {
        let (install, rom) = (self.require()?, rom(game)?);
        check_libcrypt(&install, game, rom)?;
        Ok(command(&install, &game.path, rom, settings))
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

    #[test]
    fn reads_serials_from_file_names() {
        assert_eq!(serials("Resident Evil 3 - Nemesis (E) [SLES-02529].bin.ecm"), ["SLES-02529"]);
        assert_eq!(serials("sces_015.64.bin"), ["SCES-01564"]);
        assert!(serials("Final Fantasy VII (Disc 1).cue").is_empty());
        assert!(serials("XSLES-025290").is_empty());
    }

    #[test]
    fn finds_protected_games() {
        let gamedb = "\
SLES-02529:
  name: \"Resident Evil 3 - Nemesis (Europe)\"
  traits:
    - DisableWidescreen
  libcrypt: true
SLED-02541:
  name: \"Resident Evil 3 - Nemesis (Europe) (Demo)\"
SCES-01564:
  name: \"Ape Escape (Europe)\"
  codes:
    - SCES-01564
    - SCES-01999
  libcrypt: true
  metadata:
    languages:
      - English
";
        let find = |name: &str| libcrypt_game(gamedb, &[name.to_string()]);
        let re3 = Protected { serial: "SLES-02529".into(), name: "Resident Evil 3 - Nemesis (Europe)".into() };
        assert_eq!(find("Resident Evil 3 - Nemesis (E) [SLES-02529].bin.ecm"), Some(re3));
        assert_eq!(find("Ape Escape (Europe).cue").map(|g| g.serial), Some("SCES-01564".into()));
        assert_eq!(find("Ape [SCES-01999].chd").map(|g| g.serial), Some("SCES-01999".into()));
        assert_eq!(find("Resident Evil 3 [SLED-02541].bin"), None);
        assert_eq!(find("English.cue"), None);
    }
}
