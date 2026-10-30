import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api, errorMessage, type CoverInfo, type EmulatorInfo, type ExternalGame, type Runner } from "../api";
import { EnvEditor } from "./EnvEditor";
import { EmulatorTroubleshooting, Troubleshooting } from "./Troubleshooting";
import { FolderIcon } from "../icons";
import { Banner, SimplePlayButton } from "./GameParts";
import { externalKey } from "./Sidebar";
import { refreshIcon } from "./GameIcon";

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
  /** Shows the help of its system or, for a Windows game, Wine/Proton's. */
  onHelp: () => void;
  onSaveEnv: (env: Record<string, string>) => Promise<void>;
  onSaveSettings: (settings: Record<string, string>) => Promise<void>;
  onRename: (name: string) => Promise<void>;
  /** Points the game at where its image, executable or folder is now. */
  onRelocate: (path: string) => Promise<void>;
}

/**
 * The box art of a game whose disc carries no pictures (PS1, PS2, Dreamcast): only
 * downloaded when the player asks, as Runux never connects on its own.
 */
function CoverRow({ game }: { game: ExternalGame }) {
  const [info, setInfo] = useState<CoverInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setInfo(null);
    setError(null);
    api.coverInfo(game.id).then(setInfo).catch(() => setInfo(null));
  }, [game.id, game.rom]);

  if (!info) return null;
  const run = (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    action()
      .then(() => refreshIcon(externalKey(game.id)))
      .then(() => api.coverInfo(game.id))
      .then(setInfo)
      .catch((e) => setError(errorMessage(e)))
      .finally(() => setBusy(false));
  };
  return (
    <>
      <dt>Carátula</dt>
      <dd>
        {info.downloaded ? (
          <>Descargada · <button className="link-btn" disabled={busy} onClick={() => run(() => api.removeCover(game.id))}>Quitar</button></>
        ) : (
          <>
            <button className="link-btn" disabled={busy} onClick={() => run(() => api.downloadCover(game.id))}>
              {busy ? "Descargando…" : "Descargar carátula"}
            </button>
            <small className="muted"> · De GitHub, buscándola por {info.lookup} ({info.key}), lo único que se envía.</small>
          </>
        )}
        {error && <div className="note">{error}</div>}
      </dd>
    </>
  );
}

/** What the game opens, as the player would name it. */
const opened = (game: ExternalGame) =>
  game.rom ? "la imagen del juego" : game.exe ? "el ejecutable del juego" : "la carpeta del juego";

/**
 * Asks where the game's image, executable or folder is now: a file of the
 * kinds the emulator opens, an `.exe`, or a folder.
 */
async function pickLocation(game: ExternalGame, emulator?: EmulatorInfo): Promise<string | null> {
  const defaultPath = game.missing ? undefined : game.path;
  if (!emulator) {
    return open({ defaultPath, filters: [{ name: "Ejecutables de Windows", extensions: ["exe", "EXE"] }] });
  }
  const media = emulator.media;
  if (media.type !== "file") return open({ defaultPath, directory: true });
  const isImage = media.extensions.some((e) => game.rom?.toLowerCase().endsWith(`.${e}`));
  if (media.folder && !isImage) return open({ defaultPath, directory: true });
  const extensions = [...media.extensions, ...media.extensions.map((e) => e.toUpperCase())];
  return open({ defaultPath, filters: [{ name: `Juegos de ${emulator.system}`, extensions }] });
}

const engine = (emulator?: EmulatorInfo) =>
  !emulator ? "Windows (Wine/Proton)"
  : emulator.system === emulator.program ? emulator.program
  : `${emulator.system} (${emulator.program})`;

export function ExternalGameView({
  game, emulator, runner, launching, onPlay, onOpenFolder, onRemove, onOpenEmulator, onHelp, onSaveEnv, onSaveSettings,
  onRename, onRelocate,
}: Props) {
  const [relocating, setRelocating] = useState(false);
  const [relocateError, setRelocateError] = useState<string | null>(null);
  useEffect(() => setRelocateError(null), [game.id]);

  const relocate = async () => {
    setRelocateError(null);
    try {
      const path = await pickLocation(game, emulator);
      if (!path) return;
      setRelocating(true);
      await onRelocate(path);
      refreshIcon(externalKey(game.id));
    } catch (e) {
      setRelocateError(errorMessage(e));
    } finally {
      setRelocating(false);
    }
  };
  // Files Runux imported stay in its own folder: there's nowhere else to look.
  const change = !game.imported && (
    <>
      {" · "}
      <button className="link-btn" disabled={game.running || relocating} onClick={relocate}>
        {relocating ? "Comprobando…" : "Cambiar…"}
      </button>
    </>
  );

  return (
    <div className="game-view">
      <Banner gameKey={externalKey(game.id)} name={game.name} accent={game.accent} ready onRename={onRename} />

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
        {game.missing && (
          <p className="note">
            No se encuentra {opened(game)}: <span className="mono">{game.rom ?? game.exe ?? game.path}</span>.{" "}
            {game.imported ? (
              "Quita el juego y vuelve a añadirlo."
            ) : (
              <>
                Si lo has movido,{" "}
                <button className="link-btn" disabled={relocating} onClick={relocate}>búscalo</button>.
              </>
            )}
          </p>
        )}
        {relocateError && <p className="note">{relocateError}</p>}
        {emulator && !emulator.installed && (
          <p className="note">
            {emulator.program} no está instalado.{" "}
            {emulator.installHint ? <>Instálalo con <code>{emulator.installHint}</code>.</> : "Instálalo con el gestor de paquetes de tu distribución."}
          </p>
        )}
        {emulator && (
          <p className="muted small">
            {emulator.setup && <>{emulator.setup} </>}
            <button className="link-btn" onClick={onHelp}>Ayuda de {emulator.system}</button>
          </p>
        )}
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
              <dd><span className="mono">{game.exe}</span>{change}</dd>
            </>
          )}
          {game.rom && (
            <>
              <dt>Imagen</dt>
              <dd><span className="mono">{game.rom}</span>{change}</dd>
            </>
          )}
          <dt>Carpeta</dt>
          <dd><span className="mono">{game.path}</span>{!game.rom && !game.exe && change}</dd>
          {emulator && <CoverRow game={game} />}
          <dt>Archivos</dt>
          <dd>{game.imported ? "Importados por la app (se borran al quitar el juego)" : "Carpeta propia (no se borra al quitar el juego)"}</dd>
        </dl>
        {emulator ? (
          emulator.fixes.length > 0 && (
            <EmulatorTroubleshooting key={game.id} emulator={emulator} settings={game.settings ?? {}} onSave={onSaveSettings} />
          )
        ) : (
          <>
            <Troubleshooting runner={runner} env={game.env} logKey={externalKey(game.id)} onSaveEnv={onSaveEnv} onHelp={onHelp} />
            <EnvEditor key={game.id} env={game.env} onSave={onSaveEnv} />
          </>
        )}
      </section>
    </div>
  );
}
