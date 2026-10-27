import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Runner = "proton" | "wine";

export interface Config {
  runner: Runner;
  wineBin: string;
  prefix: string;
  protonPath: string | null;
  protonData: string;
  /** Prefix of the EA app (Wine runner), kept apart from Battle.net's. */
  eaPrefix: string;
  /** `STEAM_COMPAT_DATA_PATH` of the EA app (Proton runner). */
  eaProtonData: string;
}

export interface ProtonBuild {
  name: string;
  path: string;
  version: string | null;
  runtime: string | null;
  steamRoot: string;
  /** Downloaded by the app (GE-Proton) rather than found in Steam. */
  downloaded: boolean;
  /** Steam Linux Runtime the build needs but that isn't installed. */
  missingRuntime: string | null;
}

export interface GeInstall {
  /** Release tag, e.g. `GE-Proton11-11`. */
  version: string;
  /** False when that release was already installed. */
  updated: boolean;
  /** Steam Linux Runtime downloaded along with it, if it was missing. */
  runtime: string | null;
}

export interface Status {
  runner: Runner;
  runnerLabel: string | null;
  runnerOk: boolean;
  /** Why the runner can't be used, when known. */
  runnerProblem: string | null;
  prefixExists: boolean;
  dxvkVersion: string | null;
  battlenetInstalled: boolean;
  battlenetRunning: boolean;
  scummvmInstalled: boolean;
  eaInstalled: boolean;
  /** Set when the 32-bit graphics drivers that Battle.net and the EA app need are missing. */
  missing32bit: { command: string | null } | null;
  eaRunning: boolean;
  /** The EA app's in-game overlay; `null` before anyone signs in. */
  eaOverlay: boolean | null;
}

export interface Game {
  uid: string;
  productCode: string;
  name: string;
  launchCode: string;
  accent: string;
  installPath: string;
  localPath: string;
  version: string;
  region: string;
  installed: boolean;
  playable: boolean;
  updateComplete: boolean;
  updateProgress: number | null;
  running: boolean;
}

export type ExternalKind = "scummvm" | "windows";

export interface ExternalGame {
  id: string;
  name: string;
  kind: ExternalKind;
  path: string;
  exe: string | null;
  scummvmId: string | null;
  accent: string;
  imported: boolean;
  /** Extra environment variables for the game's process. */
  env: Record<string, string>;
  running: boolean;
}

export interface EaGame {
  id: string;
  name: string;
  path: string;
  /** Content IDs it's launched by through the EA app. */
  contentIds: string[];
  /** Run directly when there are no content IDs. */
  exe: string | null;
  accent: string;
  /** Extra environment variables, passed on through the EA app. */
  env: Record<string, string>;
  running: boolean;
}

export interface AddGameRequest {
  kind: ExternalKind;
  name: string;
  sources: string[];
  exe: string | null;
}

export interface OpticalDrive {
  device: string;
  label: string | null;
  media: boolean;
}

export interface DiscInfo {
  /** ISO path or optical drive device (`/dev/sr0`). */
  source: string;
  /** Drive letter inside the prefix, while mounted. */
  letter: string | null;
  label: string | null;
  root: string | null;
}

export interface DiscSession {
  /** All mounted at once, each in its own drive. */
  discs: DiscInfo[];
  /** Installers on the first disc. */
  installers: string[];
  driveC: string;
  /** Executables added to the prefix since mounting, the likeliest game first. */
  installed: string[];
}

/** Tasks of the EA app's installer and of the client itself. */
export const EA_INSTALL_TASK = "ea-installer";
export const EA_TASK = "ea";

/** Task of a Battle.net game's uninstaller. */
export const uninstallTask = (uid: string) => `uninstall:${uid}`;

/** Task of a game's uninstaller run from the EA prefix. */
export const eaUninstallTask = (id: string) => `ea-uninstall:${id}`;

/** Task of the installer run from a disc. */
export const DISC_INSTALL_TASK = "disc-install";

export interface LogLine {
  task: string;
  line: string;
}

export interface Progress {
  task: string;
  downloaded: number;
  total: number | null;
}

export interface TaskFinished {
  task: string;
  success: boolean;
}

/** Downloading GE-Proton from Settings. */
export const GE_PROTON_TASK = "ge-proton";

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  setConfig: (config: Config) => invoke<void>("set_config", { config }),
  getStatus: () => invoke<Status>("get_status"),
  createPrefix: () => invoke<void>("create_prefix"),
  installDxvk: () => invoke<string>("install_dxvk"),
  installBattlenet: () => invoke<void>("install_battlenet"),
  launchBattlenet: () => invoke<void>("launch_battlenet"),
  closeBattlenet: () => invoke<void>("close_battlenet"),
  installEa: () => invoke<void>("install_ea"),
  launchEa: () => invoke<void>("launch_ea"),
  closeEa: () => invoke<void>("close_ea"),
  openEaPrefix: () => invoke<void>("open_ea_prefix"),
  listEaGames: () => invoke<EaGame[]>("list_ea_games"),
  launchEaGame: (id: string) => invoke<void>("launch_ea_game", { id }),
  openEaGameFolder: (id: string) => invoke<void>("open_ea_game_folder", { id }),
  uninstallEaGame: (id: string) => invoke<void>("uninstall_ea_game", { id }),
  setEaOverlay: (enabled: boolean) => invoke<void>("set_ea_overlay", { enabled }),
  setEaGameEnv: (id: string, env: Record<string, string>) => invoke<void>("set_ea_game_env", { id, env }),
  listGames: () => invoke<Game[]>("list_games"),
  launchGame: (uid: string) => invoke<void>("launch_game", { uid }),
  uninstallGame: (uid: string) => invoke<void>("uninstall_game", { uid }),
  stopWine: () => invoke<void>("stop_wine"),
  openPrefix: () => invoke<void>("open_prefix"),
  openGameFolder: (uid: string) => invoke<void>("open_game_folder", { uid }),
  /** Data URL of the icon in a game's executable, by its key in the sidebar. */
  gameIcon: (key: string) => invoke<string | null>("game_icon", { key }),
  /** Opens the folder of a game's Proton log, by its task (`ea:<id>`, `ext:<id>`). */
  openGameLogs: (key: string) => invoke<void>("open_game_logs", { key }),
  listProtons: () => invoke<ProtonBuild[]>("list_protons"),
  installGeProton: () => invoke<GeInstall>("install_ge_proton"),
  listExternalGames: () => invoke<ExternalGame[]>("list_external_games"),
  addExternalGame: (request: AddGameRequest) => invoke<ExternalGame>("add_external_game", { request }),
  removeExternalGame: (id: string) => invoke<void>("remove_external_game", { id }),
  launchExternalGame: (id: string) => invoke<void>("launch_external_game", { id }),
  openExternalFolder: (id: string) => invoke<void>("open_external_folder", { id }),
  setExternalEnv: (id: string, env: Record<string, string>) =>
    invoke<ExternalGame>("set_external_env", { id, env }),
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
