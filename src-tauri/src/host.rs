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
    /// It's NVIDIA's driver that's missing, not Mesa's.
    pub nvidia: bool,
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

/// The 32-bit Vulkan drivers installed.
struct Drivers32 {
    /// Mesa's: `libvulkan_radeon.so`, `libvulkan_intel.so`…
    mesa: bool,
    nvidia: bool,
}

fn installed_32bit_drivers() -> Drivers32 {
    let mut found = Drivers32 { mesa: false, nvidia: false };
    for entries in lib32_dirs().iter().filter_map(|dir| std::fs::read_dir(dir).ok()) {
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            found.mesa |= name.starts_with("libvulkan_") && name.contains(".so");
            found.nvidia |= name.starts_with("libGLX_nvidia.so");
        }
    }
    found
}

/// The version of NVIDIA's proprietary driver (`595.99.02`), when it's loaded.
fn nvidia_version() -> Option<String> {
    let version = std::fs::read_to_string("/sys/module/nvidia/version").ok()?;
    Some(version.trim().to_string()).filter(|v| !v.is_empty())
}

pub fn missing_32bit_graphics() -> Option<Missing32Bit> {
    let loader = Path::new("/lib/ld-linux.so.2").exists();
    let installed = installed_32bit_drivers();
    let nvidia = nvidia_version();
    // With NVIDIA's driver loaded, games run on it: PRIME offload even hides
    // the other drivers from Vulkan, so Mesa's alone doesn't help.
    let ok = match nvidia {
        Some(_) => installed.nvidia,
        None => installed.mesa || installed.nvidia,
    };
    if loader && ok {
        return None;
    }
    let distro = Distro::current();
    Some(match nvidia {
        Some(version) => Missing32Bit {
            nvidia: true,
            command: distro.map(|d| d.install_32bit_nvidia(&version)),
        },
        None => Missing32Bit { nvidia: false, command: distro.map(Distro::install_32bit_graphics) },
    })
}

/// Commands that install, on this distribution, programs Runux uses but
/// doesn't depend on; `None` when it isn't one we know.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstallHints {
    /// Wine with its 64-bit part, which Battle.net needs.
    pub wine: Option<String>,
}

pub fn install_hints() -> InstallHints {
    let distro = Distro::current();
    InstallHints {
        wine: distro.map(|d| d.install(if d == Distro::Debian { "wine wine64" } else { "wine" })),
    }
}

/// The command that installs `packages` (space-separated) on this
/// distribution, when it's one we know.
pub fn install_command(packages: &str) -> Option<String> {
    Distro::current().map(|d| d.install(packages))
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

    /// How to install the 32-bit part of NVIDIA's driver `version`.
    fn install_32bit_nvidia(self, version: &str) -> String {
        match self {
            Distro::Debian => {
                // Ubuntu packages each branch apart (`libnvidia-gl-595`); Debian, as one.
                let branch = version.split('.').next().unwrap_or(version);
                let ubuntu = Path::new(&format!("/var/lib/dpkg/info/libnvidia-gl-{branch}:amd64.list")).exists();
                let package = if ubuntu { format!("libnvidia-gl-{branch}:i386") } else { "nvidia-driver-libs:i386".into() };
                format!("sudo dpkg --add-architecture i386\nsudo apt update\nsudo apt install {package}")
            }
            // From RPM Fusion, like the 64-bit one.
            Distro::Fedora => "sudo dnf install xorg-x11-drv-nvidia-libs.i686".into(),
            Distro::Arch => "# Activa el repositorio [multilib] en /etc/pacman.conf y después:\n\
                 sudo pacman -Syu lib32-nvidia-utils"
                .into(),
            Distro::Suse => "sudo zypper install nvidia-gl-G06-32bit".into(),
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
        assert!(nobara.install_32bit_nvidia("595.99.02").contains("nvidia-libs.i686"));
        assert!(debian.install_32bit_nvidia("595.99.02").ends_with(":i386"));
        assert_eq!(Distro::parse("ID=manjaro\nID_LIKE=arch\n"), Some(Distro::Arch));
        assert_eq!(Distro::parse("ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n"), Some(Distro::Suse));
        assert_eq!(Distro::parse("ID=gentoo\n"), None);
    }
}
