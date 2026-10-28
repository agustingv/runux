//! ScummVM, which runs classic adventure games from their original files.

use super::program::{Install, Program};
use super::{option_args, Emulator, Fix, Media};
use crate::external::{ExternalGame, Kind};
use crate::task::Task;
use crate::{host, wine};
use std::collections::BTreeMap;
use std::path::Path;
use tauri::AppHandle;
use tokio::process::Command;

/// `/usr/games` may be missing from `$PATH` when the app is started from the
/// desktop, and it's where Debian/Ubuntu install it.
const PROGRAM: Program = Program { bins: &["scummvm"], paths: &["/usr/games/scummvm"], flatpak: None };

pub struct Scummvm;

/// Opens the game `target` in `dir` with `settings` as `--option[=value]`.
fn command(install: &Install, dir: &Path, target: &str, settings: &BTreeMap<String, String>) -> Command {
    let mut cmd = install.command(&[dir]);
    cmd.args(option_args(settings));
    cmd.arg(format!("--path={}", dir.display())).arg(target);
    cmd
}

impl Emulator for Scummvm {
    fn kind(&self) -> Kind {
        Kind::Scummvm
    }

    fn system(&self) -> &'static str {
        "ScummVM"
    }

    fn program(&self) -> &'static str {
        "ScummVM"
    }

    fn find(&self) -> Option<Install> {
        PROGRAM.find()
    }

    fn install_hint(&self) -> Option<String> {
        host::install_command("scummvm")
    }

    fn media(&self) -> Media {
        Media::GameFiles
    }

    fn fixes(&self) -> Vec<Fix> {
        vec![Fix::flag(
            "--fullscreen",
            "La ventana del juego es muy pequeña",
            "Abre el juego a pantalla completa. Alt+Intro cambia entre ventana y pantalla completa mientras juegas.",
        )]
    }

    fn command(&self, game: &ExternalGame, settings: &BTreeMap<String, String>) -> Result<Command, String> {
        let target = game.scummvm_id.as_deref().ok_or("Falta el identificador de ScummVM")?;
        Ok(command(&self.require()?, &game.path, target, settings))
    }

    /// The game folder, from `--path=`.
    fn running_marker(&self, game: &ExternalGame) -> String {
        format!("--path={} ", game.path.to_string_lossy().to_lowercase())
    }
}

/// Parses `scummvm --detect` output: a table whose dashed rule gives the column widths.
fn parse_detect(output: &str) -> Option<(String, String)> {
    let mut lines = output.lines();
    let rule = lines.by_ref().find(|l| l.starts_with("---"))?;
    let widths: Vec<usize> = rule.split(' ').map(str::len).collect();
    let row = lines.find(|l| !l.trim().is_empty())?;
    let id_end = (*widths.first()?).min(row.len());
    let desc_start = (id_end + 1).min(row.len());
    let desc_end = (desc_start + widths.get(1)?).min(row.len());
    let id = row.get(..id_end)?.trim();
    let desc = row.get(desc_start..desc_end).or_else(|| row.get(desc_start..))?.trim();
    (!id.is_empty()).then(|| (id.to_string(), desc.to_string()))
}

/// The game in `dir`: its ScummVM target and description.
pub async fn detect(app: &AppHandle, dir: &Path) -> Result<(String, String), String> {
    let task = Task::Import.name();
    let out = Scummvm
        .require()?
        .command(&[dir])
        .arg("--detect")
        .arg(format!("--path={}", dir.display()))
        .output()
        .await
        .map_err(|e| format!("No se pudo ejecutar ScummVM: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        wine::log(app, task, line);
    }
    parse_detect(&text).ok_or_else(|| "ScummVM no reconoce ningún juego en esos archivos".into())
}

#[cfg(test)]
mod tests {
    use super::super::program::args;
    use super::*;

    #[test]
    fn parses_detect_table() {
        let out = "\
GameID                         Description                                                Full Path
------------------------------ ---------------------------------------------------------- ---------
bladerunner:bladerunner        Blade Runner (Spanish)                                     /g/br
";
        assert_eq!(
            parse_detect(out),
            Some(("bladerunner:bladerunner".into(), "Blade Runner (Spanish)".into()))
        );
        assert_eq!(parse_detect("WARNING: no games found\n"), None);
    }

    #[test]
    fn passes_settings_before_the_game() {
        let settings = BTreeMap::from([("--fullscreen".to_string(), String::new()), ("--gfx-mode".to_string(), "opengl".to_string())]);
        let cmd = command(&Install::Native("/usr/games/scummvm".into()), Path::new("/g/BR"), "bladerunner:bladerunner", &settings);
        assert_eq!(args(&cmd), ["--fullscreen", "--gfx-mode=opengl", "--path=/g/BR", "bladerunner:bladerunner"]);
    }
}
