import type { EaGame, ExternalGame, Game } from "../api";
import { HomeIcon, PlusIcon } from "../icons";
import { GameIcon } from "./GameIcon";

interface Props {
  games: Game[];
  external: ExternalGame[];
  eaGames: EaGame[];
  selected: string;
  onSelect: (id: string) => void;
  onAddGame: () => void;
}

export const HOME = "home";
/** Selection key of an external game, kept apart from Battle.net uids. */
export const externalKey = (id: string) => `ext:${id}`;
/** Selection key of an EA game; also the task name of its launch. */
export const eaKey = (id: string) => `ea:${id}`;

export function Sidebar({ games, external, eaGames, selected, onSelect, onAddGame }: Props) {
  return (
    <nav className="sidebar">
      <button
        className={`side-item ${selected === HOME ? "active" : ""}`}
        onClick={() => onSelect(HOME)}
      >
        <span className="side-icon home"><HomeIcon size={16} /></span>
        <span className="side-label">Inicio</span>
      </button>

      <div className="side-heading">Juegos</div>
      {games.length === 0 && <div className="side-empty">Sin juegos detectados</div>}
      {games.map((g) => (
        <button
          key={g.uid}
          className={`side-item ${selected === g.uid ? "active" : ""} ${g.installed ? "" : "dim"}`}
          onClick={() => onSelect(g.uid)}
          title={g.name}
        >
          <GameIcon gameKey={g.uid} name={g.name} ready={g.installed} className="side-icon" style={{ background: g.accent }} />
          <span className="side-label">{g.name}</span>
          {g.running && <span className="dot running" title="En ejecución" />}
          {!g.running && g.updateProgress !== null && <span className="dot updating" title="Actualizando" />}
        </button>
      ))}

      {eaGames.length > 0 && <div className="side-heading">EA</div>}
      {eaGames.map((g) => (
        <button
          key={g.id}
          className={`side-item ${selected === eaKey(g.id) ? "active" : ""}`}
          onClick={() => onSelect(eaKey(g.id))}
          title={g.name}
        >
          <GameIcon gameKey={eaKey(g.id)} name={g.name} ready className="side-icon" style={{ background: g.accent }} />
          <span className="side-label">{g.name}</span>
          {g.running && <span className="dot running" title="En ejecución" />}
        </button>
      ))}

      <div className="side-heading">Otros juegos</div>
      {external.map((g) => (
        <button
          key={g.id}
          className={`side-item ${selected === externalKey(g.id) ? "active" : ""}`}
          onClick={() => onSelect(externalKey(g.id))}
          title={g.name}
        >
          <GameIcon gameKey={externalKey(g.id)} name={g.name} ready={g.kind === "windows"} className="side-icon" style={{ background: g.accent }} />
          <span className="side-label">{g.name}</span>
          {g.running && <span className="dot running" title="En ejecución" />}
        </button>
      ))}
      <button className="side-item" onClick={onAddGame}>
        <span className="side-icon home"><PlusIcon size={16} /></span>
        <span className="side-label">Añadir juego</span>
      </button>
    </nav>
  );
}
