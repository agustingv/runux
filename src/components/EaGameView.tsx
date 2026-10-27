import type { CSSProperties } from "react";
import { type EaGame, type Runner } from "../api";
import { FolderIcon, PlayIcon } from "../icons";
import { GameIcon } from "./GameIcon";
import { eaKey } from "./Sidebar";
import { EnvEditor } from "./EnvEditor";
import { Troubleshooting } from "./Troubleshooting";

interface Props {
  game: EaGame;
  runner: Runner;
  launching: boolean;
  eaRunning: boolean;
  /** The EA app's in-game overlay; `null` before anyone signs in. */
  eaOverlay: boolean | null;
  onSetEaOverlay: (enabled: boolean) => Promise<void>;
  onPlay: () => void;
  onOpenFolder: () => void;
  uninstalling: boolean;
  onUninstall: () => void;
  onSaveEnv: (env: Record<string, string>) => Promise<void>;
}

export function EaGameView({ game, runner, launching, eaRunning, eaOverlay, onSetEaOverlay, onPlay, onOpenFolder, uninstalling, onUninstall, onSaveEnv }: Props) {
  const label = game.running ? "Jugando" : launching ? "Iniciando…" : "Jugar";

  return (
    <div className="game-view">
      <header className="banner" style={{ "--accent": game.accent } as CSSProperties}>
        <GameIcon gameKey={eaKey(game.id)} name={game.name} ready className="banner-mark" />
        <h1>{game.name}</h1>
      </header>

      <section className="play-bar">
        <button className="play-btn" onClick={onPlay} disabled={game.running || launching}>
          {label === "Jugar" && <PlayIcon />}
          {label}
        </button>
        <button className="ghost-btn" onClick={onOpenFolder}>
          <FolderIcon /> Carpeta
        </button>
        <button className="ghost-btn danger" onClick={onUninstall} disabled={game.running || uninstalling}>
          {uninstalling ? "Desinstalando…" : "Desinstalar"}
        </button>
      </section>
      {uninstalling && (
        <p className="note uninstall-note">
          Sigue los pasos del desinstalador en su ventana. Al terminar, el juego desaparecerá de la lista.
        </p>
      )}

      <section className="details">
        <dl>
          <dt>Estado</dt>
          <dd>{game.running ? "En ejecución" : "Listo para jugar"}</dd>
          <dt>Lanzamiento</dt>
          <dd>
            {game.contentIds.length > 0
              ? `A través de la EA app${eaRunning ? "" : ", que se abrirá si hace falta"}`
              : "Ejecutable directo (su manifiesto no indica cómo lanzarlo a través de la EA app)."}
          </dd>
          {game.exe && (
            <>
              <dt>Ejecutable</dt>
              <dd className="mono">{game.exe}</dd>
            </>
          )}
          <dt>Carpeta</dt>
          <dd className="mono">{game.path}</dd>
        </dl>
        <Troubleshooting
          runner={runner}
          env={game.env}
          logKey={eaKey(game.id)}
          onSaveEnv={onSaveEnv}
          eaOverlay={{ enabled: eaOverlay, onSet: onSetEaOverlay }}
        />
        <EnvEditor
          key={game.id}
          env={game.env}
          onSave={onSaveEnv}
          hint={game.contentIds.length > 0
            ? "Una por línea, NOMBRE=valor. El juego las recibe a través de la EA app: si está abierta con otras, se reinicia al lanzarlo."
            : undefined}
        />
      </section>
    </div>
  );
}
