import { initials, type EaGame, type ExternalGame, type Game, type Progress, type Status } from "../api";
import { ProgressBar } from "./Setup";
import { eaKey, externalKey } from "./Sidebar";

interface Props {
  status: Status;
  games: Game[];
  external: ExternalGame[];
  eaGames: EaGame[];
  battlenetStarting: boolean;
  onOpenBattlenet: () => void;
  onCloseBattlenet: () => void;
  onSelect: (uid: string) => void;
  ea: EaProps;
}

interface EaProps {
  starting: boolean;
  installing: boolean;
  progress?: Progress;
  onInstall: () => void;
  onOpen: () => void;
  onClose: () => void;
}

function EaCard({ status, ea }: { status: Status; ea: EaProps }) {
  const description = ea.installing
    ? "Instalador en marcha: sigue los pasos en su ventana. Al terminar podrás abrirla desde aquí."
    : !status.eaInstalled
      ? "Instala el cliente oficial de Electronic Arts en un prefijo propio, aparte del de Battle.net."
      : status.eaRunning
        ? "El cliente está abierto. Instala y juega a tus juegos de EA desde su ventana."
        : "Abre el cliente para iniciar sesión, instalar juegos o aplicar actualizaciones.";
  return (
    <section className="card hero">
      <div>
        <h2>EA app</h2>
        <p className="muted">{description}</p>
        {ea.installing && <ProgressBar p={ea.progress} />}
      </div>
      {!status.eaInstalled ? (
        <button className="play-btn" onClick={ea.onInstall} disabled={ea.installing || !status.runnerOk}>
          {ea.installing ? "Instalando…" : "Instalar EA app"}
        </button>
      ) : status.eaRunning ? (
        <div className="hero-actions">
          <button className="ghost-btn danger" onClick={ea.onClose}>Cerrar</button>
          <button className="play-btn" disabled>Abierta</button>
        </div>
      ) : (
        <button className="play-btn" onClick={ea.onOpen} disabled={ea.starting}>
          {ea.starting ? "Abriendo…" : "Abrir EA app"}
        </button>
      )}
    </section>
  );
}

export function Home({ status, games, external, eaGames, battlenetStarting, onOpenBattlenet, onCloseBattlenet, onSelect, ea }: Props) {
  return (
    <div className="home">
      <section className="card hero">
        <div>
          <h2>Battle.net</h2>
          <p className="muted">
            {status.battlenetRunning
              ? "El cliente está abierto. Instala, actualiza o repara juegos desde su ventana."
              : "Abre el cliente para iniciar sesión, instalar juegos o aplicar actualizaciones."}
          </p>
        </div>
        {status.battlenetRunning ? (
          <div className="hero-actions">
            <button className="ghost-btn danger" onClick={onCloseBattlenet}>Cerrar</button>
            <button className="play-btn" disabled>Abierto</button>
          </div>
        ) : (
          <button className="play-btn" onClick={onOpenBattlenet} disabled={battlenetStarting}>
            {battlenetStarting ? "Abriendo…" : "Abrir Battle.net"}
          </button>
        )}
      </section>

      <EaCard status={status} ea={ea} />

      <h3 className="section-title">Tus juegos</h3>
      {games.length === 0 ? (
        <div className="card empty">
          <p>Todavía no hay juegos.</p>
          <p className="muted">
            Abre Battle.net, inicia sesión e instala tus juegos: aparecerán aquí
            automáticamente en cuanto el cliente los registre.
          </p>
        </div>
      ) : (
        <div className="grid">
          {games.map((g) => (
            <button key={g.uid} className="tile" onClick={() => onSelect(g.uid)}>
              <div className="tile-art" style={{ background: g.accent }}>{initials(g.name)}</div>
              <div className="tile-name">{g.name}</div>
              <div className="tile-state muted">
                {g.running ? "En ejecución"
                  : g.updateProgress !== null ? `Actualizando ${(g.updateProgress * 100).toFixed(0)}%`
                  : !g.installed ? "No instalado"
                  : g.playable ? "Listo" : "Necesita actualización"}
              </div>
            </button>
          ))}
        </div>
      )}

      {eaGames.length > 0 && (
        <>
          <h3 className="section-title">Juegos de EA</h3>
          <div className="grid">
            {eaGames.map((g) => (
              <button key={g.id} className="tile" onClick={() => onSelect(eaKey(g.id))}>
                <div className="tile-art" style={{ background: g.accent }}>{initials(g.name)}</div>
                <div className="tile-name">{g.name}</div>
                <div className="tile-state muted">{g.running ? "En ejecución" : "Listo"}</div>
              </button>
            ))}
          </div>
        </>
      )}

      {external.length > 0 && (
        <>
          <h3 className="section-title">Otros juegos</h3>
          <div className="grid">
            {external.map((g) => (
              <button key={g.id} className="tile" onClick={() => onSelect(externalKey(g.id))}>
                <div className="tile-art" style={{ background: g.accent }}>{initials(g.name)}</div>
                <div className="tile-name">{g.name}</div>
                <div className="tile-state muted">{g.running ? "En ejecución" : "Listo"}</div>
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
