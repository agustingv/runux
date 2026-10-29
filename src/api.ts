import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { CoverInfo } from "./bindings/CoverInfo";
import type { ExternalKind } from "./bindings/ExternalKind";
import type { Task } from "./bindings/Task";

// Types shared with the Rust side, generated from it by `cargo test` (ts-rs).
export type { AddGameRequest } from "./bindings/AddGameRequest";
export type { Config } from "./bindings/Config";
export type { CoverInfo } from "./bindings/CoverInfo";
export type { DiscInfo } from "./bindings/DiscInfo";
export type { DiscSession } from "./bindings/DiscSession";
export type { EaGame } from "./bindings/EaGame";
export type { EmulatorFix } from "./bindings/EmulatorFix";
export type { EmulatorInfo } from "./bindings/EmulatorInfo";
export type { HelpBlock } from "./bindings/HelpBlock";
export type { HelpSection } from "./bindings/HelpSection";
export type { ExternalGame } from "./bindings/ExternalGame";
export type { ExternalGameEntry } from "./bindings/ExternalGameEntry";
export type { ExternalKind } from "./bindings/ExternalKind";
export type { Game } from "./bindings/Game";
export type { GeInstall } from "./bindings/GeInstall";
export type { LogLine } from "./bindings/LogLine";
export type { OpticalDrive } from "./bindings/OpticalDrive";
export type { Progress } from "./bindings/Progress";
export type { ProtonBuild } from "./bindings/ProtonBuild";
export type { Runner } from "./bindings/Runner";
export type { Snapshot } from "./bindings/Snapshot";
export type { Status } from "./bindings/Status";
export type { TaskFinished } from "./bindings/TaskFinished";
export type { Task };

import type { AddGameRequest } from "./bindings/AddGameRequest";
import type { Config } from "./bindings/Config";
import type { DiscSession } from "./bindings/DiscSession";
import type { ExternalGameEntry } from "./bindings/ExternalGameEntry";
import type { GeInstall } from "./bindings/GeInstall";
import type { LogLine } from "./bindings/LogLine";
import type { OpticalDrive } from "./bindings/OpticalDrive";
import type { Progress } from "./bindings/Progress";
import type { ProtonBuild } from "./bindings/ProtonBuild";
import type { Snapshot } from "./bindings/Snapshot";
import type { TaskFinished } from "./bindings/TaskFinished";

/** The tasks the UI follows by name; `satisfies` checks them against the Rust ones. */
export const TASK = {
  prefix: "prefix",
  dxvk: "dxvk",
  battlenet: "battlenet",
  installer: "installer",
  ea: "ea",
  eaInstaller: "ea-installer",
  geProton: "ge-proton",
  import: "import",
  discInstall: "disc-install",
} as const satisfies Record<string, Task>;

/** Task of a Battle.net game's uninstaller. */
export const uninstallTask = (uid: string) => `uninstall:${uid}`;

/** Task of a game's uninstaller run from the EA prefix. */
export const eaUninstallTask = (id: string) => `ea-uninstall:${id}`;

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  setConfig: (config: Config) => invoke<void>("set_config", { config }),
  snapshot: () => invoke<Snapshot>("snapshot"),
  createPrefix: () => invoke<void>("create_prefix"),
  installDxvk: () => invoke<string>("install_dxvk"),
  installBattlenet: () => invoke<void>("install_battlenet"),
  launchBattlenet: () => invoke<void>("launch_battlenet"),
  closeBattlenet: () => invoke<void>("close_battlenet"),
  installEa: () => invoke<void>("install_ea"),
  launchEa: () => invoke<void>("launch_ea"),
  closeEa: () => invoke<void>("close_ea"),
  openEaPrefix: () => invoke<void>("open_ea_prefix"),
  launchEaGame: (id: string) => invoke<void>("launch_ea_game", { id }),
  openEaGameFolder: (id: string) => invoke<void>("open_ea_game_folder", { id }),
  uninstallEaGame: (id: string) => invoke<void>("uninstall_ea_game", { id }),
  setEaOverlay: (enabled: boolean) => invoke<void>("set_ea_overlay", { enabled }),
  setEaGameEnv: (id: string, env: Record<string, string>) => invoke<void>("set_ea_game_env", { id, env }),
  launchGame: (uid: string) => invoke<void>("launch_game", { uid }),
  uninstallGame: (uid: string) => invoke<void>("uninstall_game", { uid }),
  stopWine: () => invoke<void>("stop_wine"),
  openPrefix: () => invoke<void>("open_prefix"),
  openGameFolder: (uid: string) => invoke<void>("open_game_folder", { uid }),
  /** Data URL of the icon in a game's executable, by its key in the sidebar. */
  gameIcon: (key: string) => invoke<string | null>("game_icon", { key }),
  gameBackground: (key: string) => invoke<string | null>("game_background", { key }),
  coverInfo: (id: string) => invoke<CoverInfo | null>("cover_info", { id }),
  downloadCover: (id: string) => invoke<void>("download_cover", { id }),
  removeCover: (id: string) => invoke<void>("remove_cover", { id }),
  /** Opens the folder of a game's Proton log, by its task (`ea:<id>`, `ext:<id>`). */
  openGameLogs: (key: string) => invoke<void>("open_game_logs", { key }),
  listProtons: () => invoke<ProtonBuild[]>("list_protons"),
  installGeProton: () => invoke<GeInstall>("install_ge_proton"),
  /** Deletes a GE-Proton build downloaded by the app. */
  removeProton: (path: string) => invoke<void>("remove_proton", { path }),
  cancelDownload: (task: Task) => invoke<void>("cancel_download", { task }),
  addExternalGame: (request: AddGameRequest) => invoke<ExternalGameEntry>("add_external_game", { request }),
  removeExternalGame: (id: string) => invoke<void>("remove_external_game", { id }),
  launchExternalGame: (id: string) => invoke<void>("launch_external_game", { id }),
  openExternalFolder: (id: string) => invoke<void>("open_external_folder", { id }),
  setExternalEnv: (id: string, env: Record<string, string>) =>
    invoke<ExternalGameEntry>("set_external_env", { id, env }),
  openEmulator: (kind: ExternalKind) => invoke<void>("open_emulator", { kind }),
  setExternalSettings: (id: string, settings: Record<string, string>) =>
    invoke<ExternalGameEntry>("set_external_settings", { id, settings }),
  listOpticalDrives: () => invoke<OpticalDrive[]>("list_optical_drives"),
  startDiscSession: (discs: string[]) => invoke<DiscSession>("start_disc_session", { discs }),
  changeDisc: (index: number) => invoke<DiscSession>("change_disc", { index }),
  ejectDisc: (index: number) => invoke<DiscSession>("eject_disc", { index }),
  listIsos: (dir: string) => invoke<string[]>("list_isos", { dir }),
  discSessionInfo: () => invoke<DiscSession | null>("disc_session_info"),
  runDiscInstaller: (installer: string) => invoke<void>("run_disc_installer", { installer }),
  endDiscSession: () => invoke<void>("end_disc_session"),
};

export const events = {
  onLog: (cb: (l: LogLine) => void): Promise<UnlistenFn> =>
    listen<LogLine>("log", (e) => cb(e.payload)),
  onProgress: (cb: (p: Progress) => void): Promise<UnlistenFn> =>
    listen<Progress>("progress", (e) => cb(e.payload)),
  onTaskFinished: (cb: (t: TaskFinished) => void): Promise<UnlistenFn> =>
    listen<TaskFinished>("task-finished", (e) => cb(e.payload)),
};

export function errorMessage(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

export function initials(name: string): string {
  const skip = new Set(["of", "the", "de", "la", "del"]);
  return name
    .split(/[\s:]+/)
    .filter((w) => w && !skip.has(w.toLowerCase()))
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join("");
}

/** Sorts paths so that "Disc 2" comes before "Disc 10". */
export function naturalSort(paths: string[]): string[] {
  return [...paths].sort((a, b) => a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" }));
}

export function formatBytes(n: number): string {
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
