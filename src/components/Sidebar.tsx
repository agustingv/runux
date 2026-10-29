import { Fragment } from "react";
import type { EaGame, EmulatorInfo, ExternalGame, Game } from "../api";
import { HomeIcon, PlusIcon } from "../icons";
import { RunningDot, SideGame } from "./GameParts";

interface Props {
  games: Game[];
  external: ExternalGame[];
  emulators: EmulatorInfo[];
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

export function Sidebar({ games, external, emulators, eaGames, selected, onSelect, onAddGame }: Props) {
  const externalGame = (g: ExternalGame) => (
    <SideGame
      key={g.id}
      gameKey={externalKey(g.id)}
      name={g.name}
      accent={g.accent}
      ready
      active={selected === externalKey(g.id)}
      onSelect={() => onSelect(externalKey(g.id))}
    >
      {g.running && <RunningDot />}
    </SideGame>
  );

  return (
    <nav className="sidebar">
      <div className="side-toolbar">
        <button className={`side-tool ${selected === HOME ? "active" : ""}`} onClick={() => onSelect(HOME)}>
          <HomeIcon size={15} /> Inicio
        </button>
        <button className="side-tool" onClick={onAddGame}>
          <PlusIcon size={15} /> Añadir juego
        </button>
      </div>

      <div className="side-list">
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
        {external.filter((g) => g.kind === "windows").map(externalGame)}

        {/* Emulated games, under their system. */}
        {emulators.map((emu) => {
          const list = external.filter((g) => g.kind === emu.kind);
          return list.length > 0 && (
            <Fragment key={emu.kind}>
              <div className="side-heading">{emu.system}</div>
              {list.map(externalGame)}
            </Fragment>
          );
        })}
      </div>
    </nav>
  );
}
