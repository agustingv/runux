import type { CSSProperties } from "react";
import { initials, type Game } from "../api";
import { FolderIcon, PlayIcon } from "../icons";

interface Props {
  game: Game;
  launching: boolean;
  onPlay: () => void;
  battlenetRunning: boolean;
  onOpenBattlenet: () => void;
  onOpenFolder: () => void;
  uninstalling: boolean;
  onUninstall: () => void;
}

export function GameView({ game, launching, battlenetRunning, onPlay, onOpenBattlenet, onOpenFolder, uninstalling, onUninstall }: Props) {
  const updating = game.updateProgress !== null;

  let action: { label: string; onClick?: () => void; disabled?: boolean };
  if (game.running) action = { label: "Jugando", disabled: true };
  else if (launching) action = { label: "Iniciando…", disabled: true };
  else if (updating) action = { label: "Actualizando…", disabled: true };
  else if (!game.installed || !game.playable) {
    const label = game.installed ? "Actualizar en Battle.net" : "Instalar en Battle.net";
    // With Battle.net open there is nothing to launch: the user acts in its window.
    action = battlenetRunning ? { label, disabled: true } : { label, onClick: onOpenBattlenet };
  }
  else action = { label: "Jugar", onClick: onPlay };

  return (
    <div className="game-view">
      <header
        className="banner"
        style={{ "--accent": game.accent } as CSSProperties}
      >
        <div className="banner-mark">{initials(game.name)}</div>
        <h1>{game.name}</h1>
      </header>

      <section className="play-bar">
        <button
          className="play-btn"
          onClick={action.onClick}
          disabled={action.disabled}
        >
          {action.label === "Jugar" && <PlayIcon />}
          {action.label}
        </button>
        <button className="ghost-btn" onClick={onOpenFolder} disabled={!game.installed}>
          <FolderIcon /> Carpeta
        </button>
        {game.installed && (
          <button className="ghost-btn danger" onClick={onUninstall} disabled={game.running || updating || uninstalling}>
            {uninstalling ? "Desinstalando…" : "Desinstalar"}
          </button>
        )}

        {updating && (
          <div className="progress inline">
            <div className="progress-fill" style={{ width: `${(game.updateProgress! * 100).toFixed(1)}%` }} />
            <span>{(game.updateProgress! * 100).toFixed(0)}%</span>
          </div>
        )}
      </section>
      {uninstalling && (
        <p className="note uninstall-note">
          Sigue los pasos del desinstalador de Blizzard en su ventana. Al terminar, Battle.net dejará de mostrar el juego como instalado.
        </p>
      )}

      <section className="details">
        <dl>
          <dt>Estado</dt>
          <dd>
            {game.running ? "En ejecución"
              : updating ? "Actualizando"
              : !game.installed ? "No instalado"
              : game.playable ? "Listo para jugar"
              : "Necesita actualización"}
          </dd>
          <dt>Versión</dt>
          <dd>{game.version || "—"}</dd>
          <dt>Región</dt>
          <dd>{game.region || "—"}</dd>
          <dt>Ruta (Windows)</dt>
          <dd className="mono">{game.installPath || "—"}</dd>
          <dt>Ruta (Linux)</dt>
          <dd className="mono">{game.localPath || "—"}</dd>
          <dt>Código</dt>
          <dd className="mono">{game.uid} · launch {game.launchCode}</dd>
        </dl>
      </section>
    </div>
  );
}
