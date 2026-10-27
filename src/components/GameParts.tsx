import type { CSSProperties, ReactNode } from "react";
import { PlayIcon } from "../icons";
import { GameIcon } from "./GameIcon";

/** The header of a game's page: its icon and name over its accent color. */
export function Banner({ gameKey, name, accent, ready }: {
  gameKey: string;
  name: string;
  accent: string;
  /** False while there is no executable to read the icon from. */
  ready: boolean;
}) {
  return (
    <header className="banner" style={{ "--accent": accent } as CSSProperties}>
      <GameIcon gameKey={gameKey} name={name} ready={ready} className="banner-mark" />
      <h1>{name}</h1>
    </header>
  );
}

/** "Jugar", or what keeps it from being pressed. */
export function PlayButton({ label, onClick, disabled }: { label: string; onClick?: () => void; disabled?: boolean }) {
  return (
    <button className="play-btn" onClick={onClick} disabled={disabled}>
      {label === "Jugar" && <PlayIcon />}
      {label}
    </button>
  );
}

/** The play button of a game that is either running, starting or ready. */
export function SimplePlayButton({ running, launching, onPlay }: { running: boolean; launching: boolean; onPlay: () => void }) {
  const label = running ? "Jugando" : launching ? "Iniciando…" : "Jugar";
  return <PlayButton label={label} onClick={onPlay} disabled={running || launching} />;
}

/** A game's tile on the home page: its icon (or initials) over its accent color. */
export function GameTile({ gameKey, name, accent, ready, state, onClick }: {
  gameKey: string;
  name: string;
  accent: string;
  ready: boolean;
  state: string;
  onClick: () => void;
}) {
  return (
    <button className="tile" onClick={onClick}>
      <div className="tile-art" style={{ background: accent }}>
        <GameIcon gameKey={gameKey} name={name} ready={ready} className="tile-icon" />
      </div>
      <div className="tile-name">{name}</div>
      <div className="tile-state muted">{state}</div>
    </button>
  );
}

/** A game in the sidebar. */
export function SideGame({ gameKey, name, accent, ready, active, dim, onSelect, children }: {
  gameKey: string;
  name: string;
  accent: string;
  ready: boolean;
  active: boolean;
  dim?: boolean;
  onSelect: () => void;
  /** Status dots after the name. */
  children?: ReactNode;
}) {
  return (
    <button className={`side-item ${active ? "active" : ""} ${dim ? "dim" : ""}`} onClick={onSelect} title={name}>
      <GameIcon gameKey={gameKey} name={name} ready={ready} className="side-icon" style={{ background: accent }} />
      <span className="side-label">{name}</span>
      {children}
    </button>
  );
}

export const RunningDot = () => <span className="dot running" title="En ejecución" />;
