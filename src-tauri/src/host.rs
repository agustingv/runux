//! Checks on the host system that Wine/Proton depend on but can't bring along.

use serde::Serialize;
use std::path::Path;

/// 32-bit graphics drivers are missing: Proton's runtime borrows them from the
/// host, and without them 32-bit programs (Battle.net, the EA app, old games)
/// crash as soon as DXVK fails to create a Vulkan instance.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
    let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    Some(Missing32Bit { command: install_command(&os_release) })
}

/// How to install 32-bit Mesa on the distribution described by `/etc/os-release`.
fn install_command(os_release: &str) -> Option<String> {
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
        Some(
            "sudo dpkg --add-architecture i386\nsudo apt update\n\
             sudo apt install mesa-vulkan-drivers:i386 libgl1-mesa-dri:i386 libglx-mesa0:i386 libegl-mesa0:i386"
                .into(),
        )
    } else if is("fedora") {
        Some("sudo dnf install mesa-vulkan-drivers.i686 mesa-dri-drivers.i686 mesa-libGL.i686 mesa-libEGL.i686".into())
    } else if is("arch") {
        Some("# Activa el repositorio [multilib] en /etc/pacman.conf y después:\nsudo pacman -Syu lib32-mesa lib32-vulkan-radeon lib32-vulkan-intel".into())
    } else if is("suse") || is("opensuse") {
        Some("sudo zypper install Mesa-32bit Mesa-dri-32bit libvulkan_radeon-32bit libvulkan_intel-32bit".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_install_command_by_distribution() {
        let debian = "PRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nID=debian\n";
        assert!(install_command(debian).unwrap().contains("dpkg --add-architecture i386"));
        let mint = "ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n";
        assert!(install_command(mint).unwrap().contains("apt install"));
        let nobara = "ID=nobara\nID_LIKE=\"rhel centos fedora\"\n";
        assert!(install_command(nobara).unwrap().starts_with("sudo dnf"));
        let manjaro = "ID=manjaro\nID_LIKE=arch\n";
        assert!(install_command(manjaro).unwrap().contains("pacman"));
        let tumbleweed = "ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n";
        assert!(install_command(tumbleweed).unwrap().contains("zypper"));
        assert_eq!(install_command("ID=gentoo\n"), None);
    }
}
