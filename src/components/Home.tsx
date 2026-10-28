import { Fragment } from "react";
import { type EaGame, type ExternalGame, type Game, type Progress, type Status } from "../api";
import { Missing32Bit } from "./Missing32Bit";
import { GameTile } from "./GameParts";
import { ProgressBar } from "./Setup";
import { eaKey, externalKey } from "./Sidebar";

interface Props {
  status: Status;
  games: Game[];
  external: ExternalGame[];
  eaGames: EaGame[];
  battlenetStarting: boolean;
  battlenetInstalling: boolean;
  battlenetProgress?: Progress;
  onInstallBattlenet: () => void;
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

const readiness = (g: { running: boolean }) => (g.running ? "En ejecución" : "Listo");

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

function BattlenetCard({ status, starting, installing, progress, onInstall, onOpen, onClose }: {
  status: Status;
  starting: boolean;
  installing: boolean;
  progress?: Progress;
  onInstall: () => void;
  onOpen: () => void;
  onClose: () => void;
}) {
  const installed = status.prefixExists && status.battlenetInstalled;
  const description = installing
    ? "Instalador en marcha: sigue los pasos en su ventana. Al terminar podrás abrirlo desde aquí."
    : !installed
      ? "Instala el cliente oficial de Blizzard en un prefijo propio, aparte del de la EA app."
      : status.battlenetRunning
        ? "El cliente está abierto. Instala, actualiza o repara juegos desde su ventana."
        : "Abre el cliente para iniciar sesión, instalar juegos o aplicar actualizaciones.";
  return (
    <section className="card hero">
      <div>
        <h2>Battle.net</h2>
        <p className="muted">{description}</p>
        {installing && <ProgressBar p={progress} />}
      </div>
      {!installed ? (
        <button className="play-btn" onClick={onInstall} disabled={installing || !status.runnerOk}>
          {installing ? "Instalando…" : "Instalar Battle.net"}
        </button>
      ) : status.battlenetRunning ? (
        <div className="hero-actions">
          <button className="ghost-btn danger" onClick={onClose}>Cerrar</button>
          <button className="play-btn" disabled>Abierto</button>
        </div>
      ) : (
        <button className="play-btn" onClick={onOpen} disabled={starting}>
          {starting ? "Abriendo…" : "Abrir Battle.net"}
        </button>
      )}
    </section>
  );
}

export function Home({
  status, games, external, eaGames, battlenetStarting, battlenetInstalling, battlenetProgress,
  onInstallBattlenet, onOpenBattlenet, onCloseBattlenet, onSelect, ea,
}: Props) {
  return (
    <div className="home">
      {status.missing32bit && <Missing32Bit missing={status.missing32bit} />}
      <BattlenetCard
        status={status}
        starting={battlenetStarting}
        installing={battlenetInstalling}
        progress={battlenetProgress}
        onInstall={onInstallBattlenet}
        onOpen={onOpenBattlenet}
        onClose={onCloseBattlenet}
      />

      <EaCard status={status} ea={ea} />

      {status.battlenetInstalled && <h3 className="section-title">Juegos de Battle.net</h3>}
      {!status.battlenetInstalled ? null : games.length === 0 ? (
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
            <GameTile
              key={g.uid}
              gameKey={g.uid}
              name={g.name}
              accent={g.accent}
              ready={g.installed}
              onClick={() => onSelect(g.uid)}
              state={g.running ? "En ejecución"
                : g.updateProgress !== null ? `Actualizando ${(g.updateProgress * 100).toFixed(0)}%`
                : !g.installed ? "No instalado"
                : g.playable ? "Listo" : "Necesita actualización"}
            />
          ))}
        </div>
      )}

      {eaGames.length > 0 && (
        <>
          <h3 className="section-title">Juegos de EA</h3>
          <div className="grid">
            {eaGames.map((g) => (
              <GameTile key={g.id} gameKey={eaKey(g.id)} name={g.name} accent={g.accent} ready state={readiness(g)} onClick={() => onSelect(eaKey(g.id))} />
            ))}
          </div>
        </>
      )}

      {/* Windows games, then emulated ones under their system. */}
      {[{ kind: "windows", title: "Otros juegos" }, ...status.emulators.map((e) => ({ kind: e.kind, title: e.system }))].map(({ kind, title }) => {
        const list = external.filter((g) => g.kind === kind);
        return list.length > 0 && (
          <Fragment key={kind}>
            <h3 className="section-title">{title}</h3>
            <div className="grid">
              {list.map((g) => (
                <GameTile key={g.id} gameKey={externalKey(g.id)} name={g.name} accent={g.accent} ready={g.kind === "windows"} state={readiness(g)} onClick={() => onSelect(externalKey(g.id))} />
              ))}
            </div>
          </Fragment>
        );
      })}
    </div>
  );
}
