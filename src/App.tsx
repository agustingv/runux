import { ask } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { api, DISC_INSTALL_TASK, eaUninstallTask, uninstallTask, type EaGame, EA_INSTALL_TASK, EA_TASK, errorMessage, GE_PROTON_TASK, events, type AddGameRequest, type Config, type ExternalGame, type Game, type LogLine, type Progress, type Status } from "./api";
import { AddGame } from "./components/AddGame";
import { Console } from "./components/Console";
import { ExternalGameView } from "./components/ExternalGameView";
import { GameView } from "./components/GameView";
import { Home } from "./components/Home";
import { Settings } from "./components/Settings";
import { Setup } from "./components/Setup";
import { EaGameView } from "./components/EaGameView";
import { eaKey, externalKey, HOME, Sidebar } from "./components/Sidebar";
import { GearIcon, TerminalIcon } from "./icons";

const POLL_MS = 3000;
const MAX_LOGS = 1000;
/** How long the "Iniciando…" state lasts if the game never shows up as running. */
const LAUNCH_TIMEOUT_MS = 45_000;

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [games, setGames] = useState<Game[]>([]);
  const [external, setExternal] = useState<ExternalGame[]>([]);
  const [eaGames, setEaGames] = useState<EaGame[]>([]);
  const [showAddGame, setShowAddGame] = useState(false);
  const [selected, setSelected] = useState(HOME);
  const [busy, setBusy] = useState<Set<string>>(new Set());
  const [launching, setLaunching] = useState<Record<string, number>>({});
  const [progress, setProgress] = useState<Record<string, Progress>>({});
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [showConsole, setShowConsole] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const markBusy = useCallback((task: string, on: boolean) => {
    setBusy((prev) => {
      const next = new Set(prev);
      if (on) next.add(task);
      else next.delete(task);
      return next;
    });
  }, []);

  const refresh = useCallback(async () => {
    try {
      const s = await api.getStatus();
      setStatus(s);
      setGames(s.battlenetInstalled ? await api.listGames() : []);
      setExternal(await api.listExternalGames());
      setEaGames(s.eaInstalled ? await api.listEaGames() : []);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    api.getConfig().then(setConfig).catch((e) => setError(errorMessage(e)));
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  useEffect(() => {
    const subs = [
      events.onLog((l) => setLogs((prev) => [...prev.slice(-MAX_LOGS + 1), l])),
      events.onProgress((p) => setProgress((prev) => ({ ...prev, [p.task]: p }))),
      events.onTaskFinished((t) => {
        markBusy(t.task, false);
        if (!t.success) setError(`La tarea "${t.task}" terminó con error. Revisa el registro.`);
        refresh();
      }),
    ];
    return () => subs.forEach((p) => p.then((unlisten) => unlisten()));
  }, [markBusy, refresh]);

  // Drop the "launching" flag once the game runs or the timeout expires.
  useEffect(() => {
    const now = Date.now();
    const stale = Object.entries(launching).filter(
      ([key, t]) =>
        now - t > LAUNCH_TIMEOUT_MS ||
        games.find((g) => g.uid === key)?.running ||
        external.find((g) => externalKey(g.id) === key)?.running ||
        eaGames.find((g) => eaKey(g.id) === key)?.running,
    );
    if (stale.length) {
      setLaunching((prev) => {
        const next = { ...prev };
        stale.forEach(([uid]) => delete next[uid]);
        return next;
      });
    }
  }, [games, external, eaGames, launching]);

  /** Runs a command that finishes when the work is done; its result, or `undefined` if it failed. */
  const runTask = async <T,>(task: string, fn: () => Promise<T>): Promise<T | undefined> => {
    markBusy(task, true);
    setError(null);
    try {
      return await fn();
    } catch (e) {
      setError(errorMessage(e));
      setShowConsole(true);
    } finally {
      markBusy(task, false);
      refresh();
    }
  };

  /** Runs a command that spawns a background process; `task-finished` clears it. */
  const startTask = async (task: string, fn: () => Promise<unknown>) => {
    markBusy(task, true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      markBusy(task, false);
      setError(errorMessage(e));
    }
    refresh();
  };

  const openBattlenet = () => startTask("battlenet", api.launchBattlenet);

  const play = (uid: string) => {
    setLaunching((prev) => ({ ...prev, [uid]: Date.now() }));
    startTask(uid, () => api.launchGame(uid));
  };

  const playExternal = (id: string) => {
    setLaunching((prev) => ({ ...prev, [externalKey(id)]: Date.now() }));
    startTask(externalKey(id), () => api.launchExternalGame(id));
  };

  const playEa = (id: string) => {
    setLaunching((prev) => ({ ...prev, [eaKey(id)]: Date.now() }));
    startTask(eaKey(id), () => api.launchEaGame(id));
  };

  const uninstallBattlenetGame = async (g: Game) => {
    const ok = await ask(
      `¿Desinstalar ${g.name}? Se ejecutará el desinstalador de Blizzard, que borra el juego de ${g.installPath}.`,
      { title: "Desinstalar juego", kind: "warning" },
    );
    if (ok) startTask(uninstallTask(g.uid), () => api.uninstallGame(g.uid));
  };

  const uninstallEa = async (g: EaGame) => {
    const ok = await ask(
      `¿Desinstalar ${g.name}? Se ejecutará su desinstalador, que borra el juego de ${g.path}. Las partidas guardadas suelen estar en Documentos y no se borran con el juego.`,
      { title: "Desinstalar juego", kind: "warning" },
    );
    if (ok) startTask(eaUninstallTask(g.id), () => api.uninstallEaGame(g.id));
  };

  const addGame = async (request: AddGameRequest) => {
    markBusy("import", true);
    setError(null);
    try {
      const game = await api.addExternalGame(request);
      setShowAddGame(false);
      setSelected(externalKey(game.id));
    } catch (e) {
      setError(errorMessage(e));
      setShowConsole(true);
    } finally {
      markBusy("import", false);
      setProgress(({ import: _, ...rest }) => rest);
      refresh();
    }
  };

  const removeGame = async (g: ExternalGame) => {
    const what = g.imported ? " y sus archivos importados" : "";
    if (!(await ask(`¿Quitar ${g.name}${what}?`, { title: "Quitar juego", kind: "warning" }))) return;
    try {
      await api.removeExternalGame(g.id);
      setSelected(HOME);
      refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const saveConfig = async (c: Config) => {
    try {
      await api.setConfig(c);
      setConfig(c);
      setShowSettings(false);
      refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const guard = (fn: () => Promise<unknown>) => () => fn().catch((e) => setError(errorMessage(e)));

  // Either client is enough to start: the other can be installed from Home.
  const ready = status && ((status.prefixExists && status.battlenetInstalled) || status.eaInstalled);
  const installEa = () => startTask(EA_INSTALL_TASK, api.installEa);
  const installBattlenet = () =>
    startTask("installer", async () => {
      if (!status?.prefixExists) await api.createPrefix();
      await api.installBattlenet();
    });
  const game = games.find((g) => g.uid === selected);
  const ext = external.find((g) => externalKey(g.id) === selected);
  const eaGame = eaGames.find((g) => eaKey(g.id) === selected);

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark">R</span> Runux
        </div>
        <div className="topbar-right">
          {status && ready && (
            <span className={`pill ${status.battlenetRunning ? "on" : ""}`}>
              Battle.net {status.battlenetRunning ? "abierto" : "cerrado"}
            </span>
          )}
          {status?.eaRunning && <span className="pill on">EA app abierta</span>}
          <button
            className={`icon-btn ${showConsole ? "active" : ""}`}
            onClick={() => setShowConsole((v) => !v)}
            title="Registro"
          >
            <TerminalIcon />
          </button>
          <button className="icon-btn" onClick={() => setShowSettings(true)} title="Ajustes">
            <GearIcon />
          </button>
        </div>
      </header>

      <div className="body">
        <Sidebar
          games={games}
          external={external}
          eaGames={eaGames}
          selected={selected}
          onSelect={setSelected}
          onAddGame={() => setShowAddGame(true)}
        />

        <main className="content">
          {error && (
            <div className="toast" role="alert">
              {error}
              <button className="link-btn" onClick={() => setError(null)}>Cerrar</button>
            </div>
          )}

          {!status ? (
            <div className="muted center">Cargando…</div>
          ) : eaGame ? (
            <EaGameView
              game={eaGame}
              launching={eaKey(eaGame.id) in launching}
              runner={status.runner}
              eaRunning={status.eaRunning}
              eaOverlay={status.eaOverlay}
              onSetEaOverlay={async (enabled) => {
                await api.setEaOverlay(enabled);
                refresh();
              }}
              onPlay={() => playEa(eaGame.id)}
              onOpenFolder={guard(() => api.openEaGameFolder(eaGame.id))}
              uninstalling={busy.has(eaUninstallTask(eaGame.id))}
              onUninstall={() => uninstallEa(eaGame)}
              onSaveEnv={async (env) => {
                await api.setEaGameEnv(eaGame.id, env);
                refresh();
              }}
            />
          ) : ext ? (
            <ExternalGameView
              game={ext}
              runner={status.runner}
              launching={externalKey(ext.id) in launching}
              onPlay={() => playExternal(ext.id)}
              onOpenFolder={guard(() => api.openExternalFolder(ext.id))}
              onRemove={() => removeGame(ext)}
              onSaveEnv={async (env) => {
                await api.setExternalEnv(ext.id, env);
                refresh();
              }}
            />
          ) : !ready ? (
            <Setup
              status={status}
              busy={busy}
              progress={progress}
              onCreatePrefix={() => runTask("prefix", api.createPrefix)}
              onInstallDxvk={() => runTask("dxvk", api.installDxvk)}
              onInstallBattlenet={installBattlenet}
              onInstallEa={installEa}
              onOpenSettings={() => setShowSettings(true)}
              onInstallGeProton={() => runTask(GE_PROTON_TASK, api.installGeProton)}
            />
          ) : game ? (
            <GameView
              game={game}
              launching={game.uid in launching}
              onPlay={() => play(game.uid)}
              battlenetRunning={status.battlenetRunning}
              onOpenBattlenet={openBattlenet}
              onOpenFolder={guard(() => api.openGameFolder(game.uid))}
              uninstalling={busy.has(uninstallTask(game.uid))}
              onUninstall={() => uninstallBattlenetGame(game)}
            />
          ) : (
            <Home
              status={status}
              games={games}
              external={external}
              eaGames={eaGames}
              battlenetStarting={busy.has("battlenet") && !status.battlenetRunning}
              battlenetInstalling={busy.has("installer")}
              battlenetProgress={progress.installer}
              onInstallBattlenet={installBattlenet}
              onOpenBattlenet={openBattlenet}
              onCloseBattlenet={guard(async () => {
                await api.closeBattlenet();
                refresh();
              })}
              onSelect={setSelected}
              ea={{
                starting: busy.has(EA_TASK) && !status.eaRunning,
                installing: busy.has(EA_INSTALL_TASK),
                progress: progress[EA_INSTALL_TASK],
                onInstall: installEa,
                onOpen: () => startTask(EA_TASK, api.launchEa),
                onClose: guard(async () => {
                  await api.closeEa();
                  refresh();
                }),
              }}
            />
          )}
        </main>
      </div>

      {showConsole && <Console logs={logs} onClear={() => setLogs([])} />}

      {showAddGame && status && (
        <AddGame
          scummvmInstalled={status.scummvmInstalled}
          busy={busy.has("import")}
          installing={busy.has(DISC_INSTALL_TASK)}
          progress={progress.import}
          onAdd={addGame}
          onInstall={(installer) => startTask(DISC_INSTALL_TASK, () => api.runDiscInstaller(installer))}
          onClose={() => setShowAddGame(false)}
        />
      )}

      {showSettings && config && (
        <Settings
          config={config}
          onSave={saveConfig}
          onClose={() => setShowSettings(false)}
          onOpenPrefix={guard(api.openPrefix)}
          onOpenEaPrefix={guard(api.openEaPrefix)}
          onStopWine={guard(api.stopWine)}
          onInstallGeProton={() => runTask(GE_PROTON_TASK, api.installGeProton)}
          installingGeProton={busy.has(GE_PROTON_TASK)}
          geProtonProgress={progress[GE_PROTON_TASK]}
        />
      )}
    </div>
  );
}
