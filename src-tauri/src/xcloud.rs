//! Xbox Cloud Gaming: Game Pass games streamed from Microsoft's servers. The
//! Xbox app can't run under Wine, but its cloud games play in a browser, which
//! Microsoft supports on Linux. Runux opens them in a window of their own,
//! with a Chromium-based browser's app mode.

use crate::emulator::program::{Install, Program};
use crate::task::Task;
use serde::Serialize;
use tauri::AppHandle;
use ts_rs::TS;

pub const TASK: &str = Task::Xcloud.name();

const URL: &str = "https://www.xbox.com/play";

/// A browser that can open a page as an app, without its tabs and bars.
struct Browser {
    name: &'static str,
    program: Program,
}

/// The browsers tried, in order: Edge first, the one Microsoft supports.
const BROWSERS: &[Browser] = &[
    Browser {
        name: "Microsoft Edge",
        program: Program { bins: &["microsoft-edge-stable", "microsoft-edge"], paths: &[], flatpak: Some("com.microsoft.Edge") },
    },
    Browser {
        name: "Google Chrome",
        program: Program { bins: &["google-chrome-stable", "google-chrome"], paths: &[], flatpak: Some("com.google.Chrome") },
    },
    Browser {
        name: "Chromium",
        program: Program { bins: &["chromium", "chromium-browser"], paths: &[], flatpak: Some("org.chromium.Chromium") },
    },
    Browser {
        name: "Brave",
        program: Program { bins: &["brave-browser", "brave"], paths: &[], flatpak: Some("com.brave.Browser") },
    },
];

/// Xbox Cloud Gaming as the UI sees it.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "XcloudInfo")]
pub struct Info {
    /// The browser it opens in, if one is installed.
    pub browser: Option<String>,
    /// The command that installs the browser Microsoft supports.
    pub install_hint: String,
}

fn find() -> Option<(&'static Browser, Install)> {
    BROWSERS.iter().find_map(|b| Some((b, b.program.find()?)))
}

pub fn info() -> Info {
    Info {
        browser: find().map(|(b, _)| b.name.into()),
        install_hint: BROWSERS[0].program.flatpak_hint().unwrap_or_default(),
    }
}

/// Opens Xbox Cloud Gaming in its own window, signed in as the browser is.
pub fn launch(app: &AppHandle) -> Result<(), String> {
    let (browser, install) = find().ok_or("Falta un navegador para Xbox Cloud Gaming: instala Microsoft Edge")?;
    crate::wine::log(app, TASK, format!("Abriendo Xbox Cloud Gaming con {}", browser.name));
    let mut cmd = install.command(&[]);
    cmd.arg(format!("--app={URL}"));
    crate::wine::spawn_logged(app, TASK, cmd);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::program::args;

    #[test]
    fn edge_comes_first_and_is_suggested() {
        assert_eq!(BROWSERS[0].name, "Microsoft Edge");
        assert_eq!(info().install_hint, "flatpak install flathub com.microsoft.Edge");
    }

    #[test]
    fn flatpak_browsers_get_the_page_after_their_id() {
        let mut cmd = Install::Flatpak("com.microsoft.Edge").command(&[]);
        cmd.arg(format!("--app={URL}"));
        assert_eq!(args(&cmd), ["run", "com.microsoft.Edge", "--app=https://www.xbox.com/play"]);
    }
}
