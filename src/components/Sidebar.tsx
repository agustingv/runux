import type { EaGame, ExternalGame, Game } from "../api";
import { HomeIcon, PlusIcon } from "../icons";
import { RunningDot, SideGame } from "./GameParts";

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
        <SideGame
          key={g.uid}
          gameKey={g.uid}
          name={g.name}
          accent={g.accent}
          ready={g.installed}
          dim={!g.installed}
          active={selected === g.uid}
          onSelect={() => onSelect(g.uid)}
        >
          {g.running && <RunningDot />}
          {!g.running && g.updateProgress !== null && <span className="dot updating" title="Actualizando" />}
        </SideGame>
      ))}

      {eaGames.length > 0 && <div className="side-heading">EA</div>}
      {eaGames.map((g) => (
        <SideGame
          key={g.id}
          gameKey={eaKey(g.id)}
          name={g.name}
          accent={g.accent}
          ready
          active={selected === eaKey(g.id)}
          onSelect={() => onSelect(eaKey(g.id))}
        >
          {g.running && <RunningDot />}
        </SideGame>
      ))}

      <div className="side-heading">Otros juegos</div>
      {external.map((g) => (
        <SideGame
          key={g.id}
          gameKey={externalKey(g.id)}
          name={g.name}
          accent={g.accent}
          ready={g.kind === "windows"}
          active={selected === externalKey(g.id)}
          onSelect={() => onSelect(externalKey(g.id))}
        >
          {g.running && <RunningDot />}
        </SideGame>
      ))}
      <button className="side-item" onClick={onAddGame}>
        <span className="side-icon home"><PlusIcon size={16} /></span>
        <span className="side-label">Añadir juego</span>
      </button>
    </nav>
  );
}
