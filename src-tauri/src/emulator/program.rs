//! Finding an emulator's program, installed natively or from Flathub, and
//! starting it.

use std::path::{Path, PathBuf};
use tokio::process::Command;

pub struct Program {
    /// Executables looked for on `$PATH`.
    pub bins: &'static [&'static str],
    /// Where it may be installed off `$PATH`.
    pub paths: &'static [&'static str],
    /// Its Flathub id.
    pub flatpak: Option<&'static str>,
}

/// How a program is installed.
#[derive(Debug, PartialEq, Eq)]
pub enum Install {
    Native(PathBuf),
    Flatpak(&'static str),
}

impl Program {
    /// A native install first, else the Flatpak, per user or system-wide.
    pub fn find(&self) -> Option<Install> {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let native = std::env::split_paths(&path)
            .flat_map(|dir| self.bins.iter().map(move |bin| dir.join(bin)))
            .chain(self.paths.iter().map(PathBuf::from))
            .find(|p| p.is_file());
        if let Some(bin) = native {
            return Some(Install::Native(bin));
        }
        let id = self.flatpak?;
        let user = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/flatpak"));
        user.into_iter()
            .chain([PathBuf::from("/var/lib/flatpak")])
            .any(|root| root.join("app").join(id).join("current").exists())
            .then_some(Install::Flatpak(id))
    }

    /// The command that installs it from Flathub, the same everywhere.
    pub fn flatpak_hint(&self) -> Option<String> {
        self.flatpak.map(|id| format!("flatpak install flathub {id}"))
    }
}

impl Install {
    /// The command that starts the program, able to read `dirs`: a Flatpak
    /// only sees what its sandbox lets it, and some can't read the home folder.
    pub fn command(&self, dirs: &[&Path]) -> Command {
        match self {
            Install::Native(bin) => Command::new(bin),
            Install::Flatpak(id) => {
                let mut cmd = Command::new("flatpak");
                cmd.arg("run");
                cmd.args(dirs.iter().map(|d| format!("--filesystem={}:ro", d.display())));
                cmd.arg(id);
                cmd
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn args(cmd: &Command) -> Vec<String> {
    cmd.as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpaks_get_to_read_the_game_folder() {
        let dir = Path::new("/g/Gran Turismo");
        assert_eq!(args(&Install::Flatpak("net.pcsx2.PCSX2").command(&[dir])), ["run", "--filesystem=/g/Gran Turismo:ro", "net.pcsx2.PCSX2"]);
        assert!(args(&Install::Native("/usr/bin/pcsx2-qt".into()).command(&[dir])).is_empty());
    }
}
