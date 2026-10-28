import { type EmulatorInfo, type ExternalGame, type Runner } from "../api";
import { EnvEditor } from "./EnvEditor";
import { EmulatorTroubleshooting, Troubleshooting } from "./Troubleshooting";
import { FolderIcon } from "../icons";
import { Banner, SimplePlayButton } from "./GameParts";
import { externalKey } from "./Sidebar";

interface Props {
  game: ExternalGame;
  /** What runs the game; `undefined` for Windows games. */
  emulator?: EmulatorInfo;
  runner: Runner;
  launching: boolean;
  onPlay: () => void;
  onOpenFolder: () => void;
  onRemove: () => void;
  /** Opens the emulator on its own, to set it up. */
  onOpenEmulator: () => void;
  onSaveEnv: (env: Record<string, string>) => Promise<void>;
  onSaveSettings: (settings: Record<string, string>) => Promise<void>;
}

const engine = (emulator?: EmulatorInfo) =>
  !emulator ? "Windows (Wine/Proton)"
  : emulator.system === emulator.program ? emulator.program
  : `${emulator.system} (${emulator.program})`;

export function ExternalGameView({
  game, emulator, runner, launching, onPlay, onOpenFolder, onRemove, onOpenEmulator, onSaveEnv, onSaveSettings,
}: Props) {
  return (
    <div className="game-view">
      <Banner gameKey={externalKey(game.id)} name={game.name} accent={game.accent} ready={game.kind === "windows"} />

      <section className="play-bar">
        <SimplePlayButton running={game.running} launching={launching} onPlay={onPlay} />
        <button className="ghost-btn" onClick={onOpenFolder}>
          <FolderIcon /> Carpeta
        </button>
        {emulator?.installed && (
          <button className="ghost-btn" onClick={onOpenEmulator} title="Para configurarlo: mandos, gráficos, BIOS…">
            Abrir {emulator.program}
          </button>
        )}
        <button className="ghost-btn danger" onClick={onRemove} disabled={game.running}>
          Quitar
        </button>
      </section>

      <section className="details">
        {emulator && !emulator.installed && (
          <p className="note">
            {emulator.program} no está instalado.{" "}
            {emulator.installHint ? <>Instálalo con <code>{emulator.installHint}</code>.</> : "Instálalo con el gestor de paquetes de tu distribución."}
          </p>
        )}
        {emulator?.setup && <p className="muted small">{emulator.setup}</p>}
        <dl>
          <dt>Estado</dt>
          <dd>{game.running ? "En ejecución" : "Listo para jugar"}</dd>
          <dt>Motor</dt>
          <dd>{engine(emulator)}</dd>
          {game.scummvmId && (
            <>
              <dt>Id de ScummVM</dt>
              <dd className="mono">{game.scummvmId}</dd>
            </>
          )}
          {game.exe && (
            <>
              <dt>Ejecutable</dt>
              <dd className="mono">{game.exe}</dd>
            </>
          )}
          {game.rom && (
            <>
              <dt>Imagen</dt>
              <dd className="mono">{game.rom}</dd>
            </>
          )}
          <dt>Carpeta</dt>
          <dd className="mono">{game.path}</dd>
          <dt>Archivos</dt>
          <dd>{game.imported ? "Importados por la app (se borran al quitar el juego)" : "Carpeta propia (no se borra al quitar el juego)"}</dd>
        </dl>
        {emulator ? (
          emulator.fixes.length > 0 && (
            <EmulatorTroubleshooting key={game.id} emulator={emulator} settings={game.settings ?? {}} onSave={onSaveSettings} />
          )
        ) : (
          <>
            <Troubleshooting runner={runner} env={game.env} logKey={externalKey(game.id)} onSaveEnv={onSaveEnv} />
            <EnvEditor key={game.id} env={game.env} onSave={onSaveEnv} />
          </>
        )}
      </section>
    </div>
  );
}
