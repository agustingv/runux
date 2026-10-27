import { type ExternalGame, type Runner } from "../api";
import { EnvEditor } from "./EnvEditor";
import { Troubleshooting } from "./Troubleshooting";
import { FolderIcon } from "../icons";
import { Banner, SimplePlayButton } from "./GameParts";
import { externalKey } from "./Sidebar";

interface Props {
  game: ExternalGame;
  runner: Runner;
  launching: boolean;
  onPlay: () => void;
  onOpenFolder: () => void;
  onRemove: () => void;
  onSaveEnv: (env: Record<string, string>) => Promise<void>;
}

export function ExternalGameView({ game, runner, launching, onPlay, onOpenFolder, onRemove, onSaveEnv }: Props) {
  return (
    <div className="game-view">
      <Banner gameKey={externalKey(game.id)} name={game.name} accent={game.accent} ready={game.kind === "windows"} />

      <section className="play-bar">
        <SimplePlayButton running={game.running} launching={launching} onPlay={onPlay} />
        <button className="ghost-btn" onClick={onOpenFolder}>
          <FolderIcon /> Carpeta
        </button>
        <button className="ghost-btn danger" onClick={onRemove} disabled={game.running}>
          Quitar
        </button>
      </section>

      <section className="details">
        <dl>
          <dt>Estado</dt>
          <dd>{game.running ? "En ejecución" : "Listo para jugar"}</dd>
          <dt>Motor</dt>
          <dd>{game.kind === "scummvm" ? "ScummVM" : "Windows (Wine/Proton)"}</dd>
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
          <dt>Carpeta</dt>
          <dd className="mono">{game.path}</dd>
          <dt>Archivos</dt>
          <dd>{game.imported ? "Importados por la app (se borran al quitar el juego)" : "Carpeta propia (no se borra al quitar el juego)"}</dd>
        </dl>
        {game.kind === "windows" && <Troubleshooting runner={runner} env={game.env} logKey={externalKey(game.id)} onSaveEnv={onSaveEnv} />}
        <EnvEditor key={game.id} env={game.env} onSave={onSaveEnv} />
      </section>
    </div>
  );
}
