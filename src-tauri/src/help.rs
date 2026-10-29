//! The help pages: one per emulator, written by each (`Emulator::help`),
//! and this one for Windows games, all with the paths of this installation.

use crate::config::{self, Config, Runner};
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// A part of a help section. Text may hold `code` between backticks.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
#[ts(export)]
pub enum HelpBlock {
    Text(String),
    /// Numbered steps.
    Steps(Vec<String>),
    List(Vec<String>),
}

/// A section of a help page.
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct HelpSection {
    pub title: String,
    pub blocks: Vec<HelpBlock>,
}

pub fn section(title: &str, blocks: Vec<HelpBlock>) -> HelpSection {
    HelpSection { title: title.into(), blocks }
}

pub fn text(text: impl Into<String>) -> HelpBlock {
    HelpBlock::Text(text.into())
}

pub fn steps(items: impl IntoIterator<Item = impl Into<String>>) -> HelpBlock {
    HelpBlock::Steps(items.into_iter().map(Into::into).collect())
}

pub fn list(items: impl IntoIterator<Item = impl Into<String>>) -> HelpBlock {
    HelpBlock::List(items.into_iter().map(Into::into).collect())
}


/// `path` as the player would write it, with `~` for the home folder.
pub fn tilde(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// The help for Windows games, run with `cfg`'s runner.
pub fn windows(cfg: &Config) -> Vec<HelpSection> {
    let prefix = tilde(&cfg.wine_prefix());
    let ea_prefix = tilde(&cfg.ea().wine_prefix());
    let logs = tilde(&config::logs_dir());
    let runner = match cfg.runner {
        Runner::Proton => "Proton, la versión de Wine de Valve, ",
        Runner::Wine => "Wine ",
    };
    vec![
        section(
            "Cómo funciona",
            vec![
                text(format!(
                    "Los juegos de Windows se ejecutan con {runner}que traduce lo que el juego pide a Windows en lo \
                     que hace Linux, sin instalar Windows. Todos comparten un prefijo: una carpeta que hace de \
                     unidad C: de Windows, en `{prefix}`. Es el mismo de Battle.net; la EA app tiene el suyo, en \
                     `{ea_prefix}`."
                )),
                list([
                    "En Ajustes (la rueda dentada) eliges entre Proton, el recomendado, y Wine. Runux usa los Proton \
                     de Steam y puede descargar GE-Proton, que trae arreglos para más juegos; Steam no hace falta.",
                    "En Ajustes también puedes abrir el prefijo, para ver o copiar archivos del juego, y «Detener \
                     Wine», que cierra todo lo que se ejecuta en él si algo se queda colgado.",
                ]),
            ],
        ),
        section(
            "Añadir un juego",
            vec![
                text("En «Añadir juego», con Windows (Wine/Proton):"),
                list([
                    "«Instalar desde disco»: elige las imágenes ISO del juego (una carpeta con todas, o una a una) o \
                     el DVD del lector. Runux las monta a la vez, cada una como una unidad de CD dentro del prefijo, \
                     y abre el instalador. Al terminar, elige el `.exe` del juego entre los que ha instalado.",
                    "«Ya instalado»: elige el `.exe` de un juego que ya tengas, dentro del prefijo o en cualquier \
                     otra carpeta.",
                    "Los discos de Xbox y Xbox 360 no se pueden leer en un lector de PC, aunque el juego también \
                     exista para Windows: Runux te avisa al montarlos.",
                ]),
            ],
        ),
        section(
            "Antes de jugar",
            vec![list([
                "Muchos juegos y lanzadores son programas de 32 bits y necesitan la parte de 32 bits del controlador \
                 gráfico. Si falta, la pantalla de inicio te avisa y te da el comando para instalarla.",
                "Los juegos que piden el disco para arrancar suelen usar protecciones antiguas (SafeDisc, SecuROM) \
                 que no funcionan con Wine/Proton. Suele haber un parche oficial del juego que quita esa comprobación.",
            ])],
        ),
        section(
            "Si algo falla",
            vec![
                text(
                    "En la página de cada juego, «Solución de problemas» tiene interruptores para los fallos más \
                     comunes: el ratón que no responde en los menús, cierres en procesadores con muchos núcleos y, \
                     con Proton, un registro detallado. Se aplican solo a ese juego.",
                ),
                list([
                    "El botón del terminal, arriba a la derecha, muestra lo que escriben Wine o Proton y el juego, y \
                     se abre solo cuando algo falla."
                        .to_string(),
                    format!(
                        "Con el registro de Proton activado, cada juego guarda `steam-0.log` en una carpeta propia \
                         dentro de `{logs}`. Desactívalo al terminar: crece mucho."
                    ),
                    "Debajo, las variables de entorno del juego: una `NOMBRE=valor` por línea, que se aplican solo a \
                     él. Por ejemplo, `DXVK_HUD=fps` muestra los fotogramas por segundo."
                        .into(),
                    "Si el juego no reconoce el mando, puede que solo admita teclado y ratón: AntiMicroX convierte \
                     el mando en teclas y ratón."
                        .into(),
                ]),
            ],
        ),
    ]
}

/// Fails on text that would show wrong: code left open by an odd number of
/// backticks.
#[cfg(test)]
pub fn check(help: &[HelpSection]) {
    for block in help.iter().flat_map(|s| &s.blocks) {
        let lines = match block {
            HelpBlock::Text(t) => std::slice::from_ref(t),
            HelpBlock::Steps(l) | HelpBlock::List(l) => l.as_slice(),
        };
        assert!(lines.iter().all(|l| l.matches('`').count() % 2 == 0), "{lines:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_help_shows_where_things_are() {
        for runner in [Runner::Proton, Runner::Wine] {
            let cfg = Config { runner, prefix: "/pfx".into(), proton_data: "/proton".into(), ..Default::default() };
            let help = windows(&cfg);
            check(&help);
            let HelpBlock::Text(intro) = &help[0].blocks[0] else { unreachable!() };
            assert!(intro.contains(if runner == Runner::Proton { "`/proton/pfx`" } else { "`/pfx`" }), "{intro}");
        }
    }

    #[test]
    fn writes_home_as_tilde() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(tilde(&home.join(".var/app/x")), "~/.var/app/x");
        assert_eq!(tilde(&home), "~");
        assert_eq!(tilde(Path::new("/usr/games")), "/usr/games");
    }
}
