import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api, eaUninstallTask, errorMessage, TASK, uninstallTask, type AddGameRequest, type Config, type EaGame, type ExternalGame, type Game } from "./api";
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
import { useLauncher } from "./useLauncher";

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [showAddGame, setShowAddGame] = useState(false);
  const [selected, setSelected] = useState(HOME);
  const [showConsole, setShowConsole] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const {
    status, games, external, eaGames, busy, launching, progress, logs, error,
    setError, clearLogs, refresh, runTask, startTask, play, cancel, clearProgress, guard,
  } = useLauncher({ onTaskError: () => setShowConsole(true) });

  useEffect(() => {
    api.getConfig().then(setConfig).catch((e) => setError(errorMessage(e)));
  }, [setError]);

  const openBattlenet = () => startTask(TASK.battlenet, api.launchBattlenet);

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
    const game = await runTask(TASK.import, () => api.addExternalGame(request));
    clearProgress(TASK.import);
    if (game) {
      setShowAddGame(false);
      setSelected(externalKey(game.id));
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

  // Either client is enough to start: the other can be installed from Home.
  const ready = status && ((status.prefixExists && status.battlenetInstalled) || status.eaInstalled);
  const installEa = () => startTask(TASK.eaInstaller, api.installEa);
  const installBattlenet = () =>
    startTask(TASK.installer, async () => {
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
          emulators={status?.emulators ?? []}
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
              onPlay={() => play(eaKey(eaGame.id), () => api.launchEaGame(eaGame.id))}
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
              emulator={status.emulators.find((e) => e.kind === ext.kind)}
              runner={status.runner}
              launching={externalKey(ext.id) in launching}
              onPlay={() => play(externalKey(ext.id), () => api.launchExternalGame(ext.id))}
              onOpenFolder={guard(() => api.openExternalFolder(ext.id))}
              onRemove={() => removeGame(ext)}
              onOpenEmulator={guard(() => api.openEmulator(ext.kind))}
              onSaveEnv={async (env) => {
                await api.setExternalEnv(ext.id, env);
                refresh();
              }}
              onSaveSettings={async (settings) => {
                await api.setExternalSettings(ext.id, settings);
                refresh();
              }}
            />
          ) : !ready ? (
            <Setup
              status={status}
              busy={busy}
              progress={progress}
              onCreatePrefix={() => runTask(TASK.prefix, api.createPrefix)}
              onInstallDxvk={() => runTask(TASK.dxvk, api.installDxvk)}
              onInstallBattlenet={installBattlenet}
              onInstallEa={installEa}
              onOpenSettings={() => setShowSettings(true)}
              onInstallGeProton={() => runTask(TASK.geProton, api.installGeProton)}
              onCancelGeProton={() => cancel(TASK.geProton)}
            />
          ) : game ? (
            <GameView
              game={game}
              launching={game.uid in launching}
              onPlay={() => play(game.uid, () => api.launchGame(game.uid))}
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
              battlenetStarting={busy.has(TASK.battlenet) && !status.battlenetRunning}
              battlenetInstalling={busy.has(TASK.installer)}
              battlenetProgress={progress[TASK.installer]}
              onInstallBattlenet={installBattlenet}
              onOpenBattlenet={openBattlenet}
              onCloseBattlenet={guard(async () => {
                await api.closeBattlenet();
                refresh();
              })}
              onSelect={setSelected}
              ea={{
                starting: busy.has(TASK.ea) && !status.eaRunning,
                installing: busy.has(TASK.eaInstaller),
                progress: progress[TASK.eaInstaller],
                onInstall: installEa,
                onOpen: () => startTask(TASK.ea, api.launchEa),
                onClose: guard(async () => {
                  await api.closeEa();
                  refresh();
                }),
              }}
            />
          )}
        </main>
      </div>

      {showConsole && <Console logs={logs} onClear={clearLogs} />}

      {showAddGame && status && (
        <AddGame
          emulators={status.emulators}
          busy={busy.has(TASK.import)}
          installing={busy.has(TASK.discInstall)}
          progress={progress[TASK.import]}
          onAdd={addGame}
          onInstall={(installer) => startTask(TASK.discInstall, () => api.runDiscInstaller(installer))}
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
          onInstallGeProton={() => runTask(TASK.geProton, api.installGeProton)}
          onCancelGeProton={() => cancel(TASK.geProton)}
          installingGeProton={busy.has(TASK.geProton)}
          geProtonProgress={progress[TASK.geProton]}
        />
      )}
    </div>
  );
}
