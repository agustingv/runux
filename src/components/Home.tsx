import { type ReactNode } from "react";
import { type EaGame, type ExternalGame, type Game, type Progress, type Status } from "../api";
import { ChevronIcon } from "../icons";
import { useStored } from "../recent";
import { Missing32Bit } from "./Missing32Bit";
import { GameTile } from "./GameParts";
import { ProgressBar } from "./Setup";
import { eaKey, externalKey } from "./Sidebar";

interface Props {
  status: Status;
  games: Game[];
  external: ExternalGame[];
  eaGames: EaGame[];
  /** When each game was last seen running, by its key. */
  played: Record<string, number>;
  battlenetStarting: boolean;
  battlenetInstalling: boolean;
  battlenetProgress?: Progress;
  onInstallBattlenet: () => void;
  onOpenBattlenet: () => void;
  onCloseBattlenet: () => void;
  onSelect: (uid: string) => void;
  ea: ClientProps;
}

interface ClientProps {
  starting: boolean;
  installing: boolean;
  progress?: Progress;
  onInstall: () => void;
  onOpen: () => void;
  onClose: () => void;
}

/** How many games the recently played row shows. */
const RECENT = 6;

/** A game as a tile shows it, whatever its store or system. */
interface Tile {
  key: string;
  name: string;
  accent: string;
  ready: boolean;
  running: boolean;
  state: string;
}

const readiness = (g: { running: boolean }) => (g.running ? "En ejecución" : "Listo");

/** A client in a compact card: what it's doing and the one thing to do next. */
function ClientCard({ name, installed, running, client, description, openedLabel, canInstall }: {
  name: string;
  installed: boolean;
  running: boolean;
  client: ClientProps;
  description: string;
  openedLabel: string;
  canInstall: boolean;
}) {
  return (
    <section className={`card client ${running ? "on" : ""}`}>
      <div className="client-text">
        <h2>{name}</h2>
        <p className="muted small">{description}</p>
        {client.installing && <ProgressBar p={client.progress} />}
      </div>
      {!installed ? (
        <button className="play-btn small" onClick={client.onInstall} disabled={client.installing || !canInstall}>
          {client.installing ? "Instalando…" : "Instalar"}
        </button>
      ) : running ? (
        <div className="hero-actions">
          <button className="ghost-btn danger" onClick={client.onClose}>Cerrar</button>
          <button className="play-btn small" disabled>{openedLabel}</button>
        </div>
      ) : (
        <button className="play-btn small" onClick={client.onOpen} disabled={client.starting}>
          {client.starting ? "Abriendo…" : "Abrir"}
        </button>
      )}
    </section>
  );
}

function Clients({ status, battlenet, ea }: { status: Status; battlenet: ClientProps; ea: ClientProps }) {
  const battlenetInstalled = status.prefixExists && status.battlenetInstalled;
  return (
    <div className="clients">
      <ClientCard
        name="Battle.net"
        installed={battlenetInstalled}
        running={status.battlenetRunning}
        client={battlenet}
        canInstall={status.runnerOk}
        openedLabel="Abierto"
        description={battlenet.installing ? "Sigue los pasos en la ventana del instalador."
          : !battlenetInstalled ? "El cliente de Blizzard, en un prefijo propio."
          : status.battlenetRunning ? "Abierto: instala, actualiza o repara juegos desde su ventana."
          : "Para iniciar sesión, instalar juegos o actualizarlos."}
      />
      <ClientCard
        name="EA app"
        installed={status.eaInstalled}
        running={status.eaRunning}
        client={ea}
        canInstall={status.runnerOk}
        openedLabel="Abierta"
        description={ea.installing ? "Sigue los pasos en la ventana del instalador."
          : !status.eaInstalled ? "El cliente de Electronic Arts, en un prefijo propio."
          : status.eaRunning ? "Abierta: instala y juega a tus juegos de EA desde su ventana."
          : "Para iniciar sesión, instalar juegos o actualizarlos."}
      />
    </div>
  );
}

/** A group of games under a title that folds it away. */
function Section({ title, count, folded, onToggle, children }: {
  title: string;
  count?: number;
  folded: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <section className={`section ${folded ? "folded" : ""}`}>
      <button className="section-title" onClick={onToggle} aria-expanded={!folded}>
        <ChevronIcon size={14} />
        {title}
        {count !== undefined && <span className="section-count">{count}</span>}
      </button>
      {!folded && children}
    </section>
  );
}

function Grid({ tiles, onSelect }: { tiles: Tile[]; onSelect: (key: string) => void }) {
  return (
    <div className="grid">
      {tiles.map((t) => (
        <GameTile key={t.key} gameKey={t.key} name={t.name} accent={t.accent} ready={t.ready} state={t.state} onClick={() => onSelect(t.key)} />
      ))}
    </div>
  );
}

export function Home({
  status, games, external, eaGames, played, battlenetStarting, battlenetInstalling, battlenetProgress,
  onInstallBattlenet, onOpenBattlenet, onCloseBattlenet, onSelect, ea,
}: Props) {
  // The sections folded away, remembered between sessions.
  const [folded, setFolded] = useStored<string[]>("home-folded", []);
  const section = (id: string) => ({
    folded: folded.includes(id),
    onToggle: () => setFolded((prev) => (prev.includes(id) ? prev.filter((f) => f !== id) : [...prev, id])),
  });

  const battlenetTiles: Tile[] = games.map((g) => ({
    key: g.uid,
    name: g.name,
    accent: g.accent,
    ready: g.installed,
    running: g.running,
    state: g.running ? "En ejecución"
      : g.updateProgress !== null ? `Actualizando ${(g.updateProgress * 100).toFixed(0)}%`
      : !g.installed ? "No instalado"
      : g.playable ? "Listo" : "Necesita actualización",
  }));
  const eaTiles: Tile[] = eaGames.map((g) => ({
    key: eaKey(g.id), name: g.name, accent: g.accent, ready: true, running: g.running, state: readiness(g),
  }));
  // Windows games, then emulated ones under their system.
  const others = [{ kind: "windows", title: "Otros juegos" }, ...status.emulators.map((e) => ({ kind: e.kind, title: e.system }))]
    .map(({ kind, title }) => ({
      kind,
      title,
      tiles: external.filter((g) => g.kind === kind).map((g): Tile => ({
        key: externalKey(g.id), name: g.name, accent: g.accent, ready: true, running: g.running, state: readiness(g),
      })),
    }))
    .filter((s) => s.tiles.length > 0);

  // Running games first, then the last ones played.
  const recent = [...battlenetTiles, ...eaTiles, ...others.flatMap((s) => s.tiles)]
    .filter((t) => t.running || played[t.key])
    .sort((a, b) => Number(b.running) - Number(a.running) || (played[b.key] ?? 0) - (played[a.key] ?? 0))
    .slice(0, RECENT);

  return (
    <div className="home">
      {status.missing32bit && <Missing32Bit missing={status.missing32bit} />}
      <Clients
        status={status}
        battlenet={{
          starting: battlenetStarting,
          installing: battlenetInstalling,
          progress: battlenetProgress,
          onInstall: onInstallBattlenet,
          onOpen: onOpenBattlenet,
          onClose: onCloseBattlenet,
        }}
        ea={ea}
      />

      {recent.length > 0 && (
        <Section title="Jugados recientemente" {...section("recent")}>
          <Grid tiles={recent} onSelect={onSelect} />
        </Section>
      )}

      {status.battlenetInstalled && (
        <Section title="Juegos de Battle.net" count={games.length} {...section("battlenet")}>
          {games.length === 0 ? (
            <div className="card empty">
              <p>Todavía no hay juegos.</p>
              <p className="muted">
                Abre Battle.net, inicia sesión e instala tus juegos: aparecerán aquí
                automáticamente en cuanto el cliente los registre.
              </p>
            </div>
          ) : (
            <Grid tiles={battlenetTiles} onSelect={onSelect} />
          )}
        </Section>
      )}

      {eaTiles.length > 0 && (
        <Section title="Juegos de EA" count={eaTiles.length} {...section("ea")}>
          <Grid tiles={eaTiles} onSelect={onSelect} />
        </Section>
      )}

      {others.map(({ kind, title, tiles }) => (
        <Section key={kind} title={title} count={tiles.length} {...section(kind)}>
          <Grid tiles={tiles} onSelect={onSelect} />
        </Section>
      ))}
    </div>
  );
}
