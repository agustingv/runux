//! Checks on the host system that Wine/Proton depend on but can't bring along.

use serde::Serialize;
use std::path::Path;
use std::sync::OnceLock;
use ts_rs::TS;

/// 32-bit graphics drivers are missing: Proton's runtime borrows them from the
/// host, and without them 32-bit programs (Battle.net, the EA app, old games)
/// crash as soon as DXVK fails to create a Vulkan instance.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Missing32Bit {
    /// Commands that install them on this distribution, when known.
    pub command: Option<String>,
}

/// Dirs where distributions keep 32-bit libraries: Debian/Ubuntu multiarch,
/// Arch's `lib32`, and Fedora/openSUSE's `lib` next to `lib64`.
fn lib32_dirs() -> Vec<&'static Path> {
    let mut dirs = vec![Path::new("/usr/lib/i386-linux-gnu"), Path::new("/usr/lib32")];
    if Path::new("/usr/lib64").is_dir() && !Path::new("/usr/lib64").is_symlink() {
        dirs.push(Path::new("/usr/lib"));
    }
    dirs
}

/// A 32-bit Vulkan driver: Mesa's (`libvulkan_radeon.so`…) or NVIDIA's.
fn has_32bit_vulkan_driver() -> bool {
    lib32_dirs().iter().any(|dir| {
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries.flatten().any(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                (name.starts_with("libvulkan_") && name.contains(".so")) || name.starts_with("libGLX_nvidia.so")
            })
        })
    })
}

pub fn missing_32bit_graphics() -> Option<Missing32Bit> {
    let loader = Path::new("/lib/ld-linux.so.2").exists();
    if loader && has_32bit_vulkan_driver() {
        return None;
    }
    Some(Missing32Bit { command: Distro::current().map(Distro::install_32bit_graphics) })
}

/// Commands that install, on this distribution, programs Runux uses but
/// doesn't depend on; `None` when it isn't one we know.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstallHints {
    pub scummvm: Option<String>,
    /// Wine with its 64-bit part, which Battle.net needs.
    pub wine: Option<String>,
}

pub fn install_hints() -> InstallHints {
    let distro = Distro::current();
    InstallHints {
        scummvm: distro.map(|d| d.install("scummvm")),
        wine: distro.map(|d| d.install(if d == Distro::Debian { "wine wine64" } else { "wine" })),
    }
}

/// The families of distributions whose package names we know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Distro {
    /// Debian, Ubuntu, Mint…
    Debian,
    /// Fedora, Nobara…
    Fedora,
    /// Arch, Manjaro…
    Arch,
    Suse,
}

impl Distro {
    /// This system's, read once from `/etc/os-release`.
    fn current() -> Option<Distro> {
        static CURRENT: OnceLock<Option<Distro>> = OnceLock::new();
        *CURRENT.get_or_init(|| Distro::parse(&std::fs::read_to_string("/etc/os-release").unwrap_or_default()))
    }

    /// The family of the distribution described by an `/etc/os-release`.
    fn parse(os_release: &str) -> Option<Distro> {
        let field = |key: &str| {
            os_release
                .lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
                .map(|v| v.trim_matches('"').to_lowercase())
                .unwrap_or_default()
        };
        let ids = format!("{} {}", field("ID"), field("ID_LIKE"));
        let is = |name: &str| ids.split_whitespace().any(|id| id == name);
        if is("debian") || is("ubuntu") {
            Some(Distro::Debian)
        } else if is("fedora") {
            Some(Distro::Fedora)
        } else if is("arch") {
            Some(Distro::Arch)
        } else if is("suse") || is("opensuse") {
            Some(Distro::Suse)
        } else {
            None
        }
    }

    /// The command that installs `packages` (space-separated).
    fn install(self, packages: &str) -> String {
        match self {
            Distro::Debian => format!("sudo apt install {packages}"),
            Distro::Fedora => format!("sudo dnf install {packages}"),
            Distro::Arch => format!("sudo pacman -S {packages}"),
            Distro::Suse => format!("sudo zypper install {packages}"),
        }
    }

    /// How to install 32-bit Mesa.
    fn install_32bit_graphics(self) -> String {
        match self {
            Distro::Debian => "sudo dpkg --add-architecture i386\nsudo apt update\n\
                 sudo apt install mesa-vulkan-drivers:i386 libgl1-mesa-dri:i386 libglx-mesa0:i386 libegl-mesa0:i386"
                .into(),
            Distro::Fedora => {
                "sudo dnf install mesa-vulkan-drivers.i686 mesa-dri-drivers.i686 mesa-libGL.i686 mesa-libEGL.i686".into()
            }
            Distro::Arch => "# Activa el repositorio [multilib] en /etc/pacman.conf y después:\n\
                 sudo pacman -Syu lib32-mesa lib32-vulkan-radeon lib32-vulkan-intel"
                .into(),
            Distro::Suse => "sudo zypper install Mesa-32bit Mesa-dri-32bit libvulkan_radeon-32bit libvulkan_intel-32bit".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_distributions_and_their_commands() {
        let debian = Distro::parse("PRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nID=debian\n").unwrap();
        assert!(debian.install_32bit_graphics().contains("dpkg --add-architecture i386"));
        assert_eq!(debian.install("scummvm"), "sudo apt install scummvm");
        assert_eq!(Distro::parse("ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n"), Some(Distro::Debian));
        let nobara = Distro::parse("ID=nobara\nID_LIKE=\"rhel centos fedora\"\n").unwrap();
        assert!(nobara.install_32bit_graphics().starts_with("sudo dnf"));
        assert_eq!(Distro::parse("ID=manjaro\nID_LIKE=arch\n"), Some(Distro::Arch));
        assert_eq!(Distro::parse("ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n"), Some(Distro::Suse));
        assert_eq!(Distro::parse("ID=gentoo\n"), None);
    }
}
