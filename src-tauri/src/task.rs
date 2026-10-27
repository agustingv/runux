//! Tasks the UI follows by name: their log lines, download progress and
//! `task-finished` event. Exported to TypeScript so the names can't drift
//! apart. Games' tasks are built from their ids instead (`ea:<id>`,
//! `uninstall:<uid>`…).

use serde::Serialize;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum Task {
    /// Creating the main prefix.
    Prefix,
    Dxvk,
    /// Battle.net while it runs.
    Battlenet,
    /// Battle.net's installer.
    Installer,
    /// The EA app while it runs.
    Ea,
    EaInstaller,
    GeProton,
    /// Adding a game, extracting its ISOs.
    Import,
    /// An installer run from a disc.
    DiscInstall,
}

impl Task {
    /// The name in log lines and events, as serialized.
    pub const fn name(self) -> &'static str {
        match self {
            Task::Prefix => "prefix",
            Task::Dxvk => "dxvk",
            Task::Battlenet => "battlenet",
            Task::Installer => "installer",
            Task::Ea => "ea",
            Task::EaInstaller => "ea-installer",
            Task::GeProton => "ge-proton",
            Task::Import => "import",
            Task::DiscInstall => "disc-install",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_the_exported_ones() {
        use Task::*;
        for task in [Prefix, Dxvk, Battlenet, Installer, Ea, EaInstaller, GeProton, Import, DiscInstall] {
            assert_eq!(serde_json::to_value(task).unwrap(), task.name());
        }
    }
}
