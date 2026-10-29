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
        flatpak_dir(id).map(|_| Install::Flatpak(id))
    }

    /// How it's installed or, for its help, how it would be: from Flathub.
    pub fn find_or_flatpak(&self) -> Option<Install> {
        self.find().or(self.flatpak.map(Install::Flatpak))
    }

    /// The command that installs it from Flathub, the same everywhere.
    pub fn flatpak_hint(&self) -> Option<String> {
        self.flatpak.map(|id| format!("flatpak install flathub {id}"))
    }
}

/// Where the Flatpak `id` is installed, per user or system-wide.
fn flatpak_dir(id: &str) -> Option<PathBuf> {
    let user = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/flatpak"));
    user.into_iter()
        .chain([PathBuf::from("/var/lib/flatpak")])
        .map(|root| root.join("app").join(id).join("current"))
        .find(|dir| dir.exists())
}

impl Install {
    /// The folder holding the program's own files: the Flatpak's `/app`, or
    /// the one with the binary.
    pub fn files(&self) -> Option<PathBuf> {
        match self {
            Install::Native(bin) => bin.parent().map(Path::to_path_buf),
            Install::Flatpak(id) => flatpak_dir(id).map(|dir| dir.join("active/files")),
        }
    }

    /// Whether it's the Flatpak, which only sees its own folders and the
    /// game's: files it needs (a BIOS) must go in its folder.
    pub fn is_flatpak(&self) -> bool {
        matches!(self, Install::Flatpak(_))
    }

    /// The program's settings folder `name`, which a Flatpak keeps in its own
    /// `~/.var/app/<id>/config`.
    pub fn config_dir(&self, name: &str) -> Option<PathBuf> {
        self.xdg_dir("config", "XDG_CONFIG_HOME", ".config", name)
    }

    /// Its data folder `name`, as `config_dir`.
    pub fn data_dir(&self, name: &str) -> Option<PathBuf> {
        self.xdg_dir("data", "XDG_DATA_HOME", ".local/share", name)
    }

    /// Its cache folder `name`, as `config_dir`.
    pub fn cache_dir(&self, name: &str) -> Option<PathBuf> {
        self.xdg_dir("cache", "XDG_CACHE_HOME", ".cache", name)
    }

    /// The XDG folder `name`: in `~/.var/app/<id>/<flatpak>` for a Flatpak,
    /// else in `$<var>` or its default, `~/<default>`.
    fn xdg_dir(&self, flatpak: &str, var: &str, default: &str, name: &str) -> Option<PathBuf> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        Some(match self {
            Install::Flatpak(id) => home.join(".var/app").join(id).join(flatpak).join(name),
            Install::Native(_) => match std::env::var_os(var) {
                Some(dir) => PathBuf::from(dir).join(name),
                None => home.join(default).join(name),
            },
        })
    }

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
