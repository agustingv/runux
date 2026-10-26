import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { api, errorMessage, naturalSort, type AddGameRequest, type DiscSession, type ExternalKind, type OpticalDrive, type Progress } from "../api";
import { CloseIcon } from "../icons";
import { ProgressBar } from "./Setup";

interface Props {
  scummvmInstalled: boolean;
  busy: boolean;
  /** An installer launched from a disc is running. */
  installing: boolean;
  progress?: Progress;
  onAdd: (request: AddGameRequest) => void;
  onInstall: (installer: string) => void;
  onClose: () => void;
}

type WindowsSource = "exe" | "disc";

const fileName = (p: string) => p.split("/").pop() ?? p;
const isDevice = (disc: string) => disc.startsWith("/dev/");
/** `C:\…` form of a path inside the prefix's `drive_c`. */
const winPath = (path: string, driveC: string) =>
  path.startsWith(driveC + "/") ? "C:" + path.slice(driveC.length).split("/").join("\\") : path;

export function AddGame({ scummvmInstalled, busy, installing, progress, onAdd, onInstall, onClose }: Props) {
  const [kind, setKind] = useState<ExternalKind>("scummvm");
  const [winSource, setWinSource] = useState<WindowsSource>("exe");
  const [name, setName] = useState("");
  const [sources, setSources] = useState<string[]>([]);
  const [exe, setExe] = useState<string | null>(null);
  const [discs, setDiscs] = useState<string[]>([]);
  const [drives, setDrives] = useState<OpticalDrive[]>([]);
  const [session, setSession] = useState<DiscSession | null>(null);
  const [installer, setInstaller] = useState("");
  const [discBusy, setDiscBusy] = useState(false);
  const [discError, setDiscError] = useState<string | null>(null);
  /** The installer is done, even if some of its processes are still around. */
  const [installDone, setInstallDone] = useState(false);
  const wasInstalling = useRef(false);

  const discMode = kind === "windows" && winSource === "disc";

  useEffect(() => {
    if (discMode) api.listOpticalDrives().then(setDrives).catch(() => setDrives([]));
  }, [discMode]);

  // Unmount the disc when the dialog goes away.
  useEffect(() => () => void api.endDiscSession(), []);

  const pickIsos = async () => {
    const picked = await open({ multiple: true, filters: [{ name: "Imágenes ISO", extensions: ["iso", "ISO"] }] });
    if (picked) setSources(picked.sort());
  };
  const pickFolder = async () => {
    const picked = await open({ directory: true });
    if (picked) setSources([picked]);
  };
  const pickExe = async () => {
    const picked = await open({
      defaultPath: session?.driveC,
      filters: [{ name: "Ejecutables de Windows", extensions: ["exe", "EXE"] }],
    });
    if (picked) setExe(picked);
  };
  const pickDiscIsos = async () => {
    const picked = await open({ multiple: true, filters: [{ name: "Imágenes ISO", extensions: ["iso", "ISO"] }] });
    if (picked) setDiscs(naturalSort(picked));
  };
  const pickDiscFolder = async () => {
    const picked = await open({ directory: true });
    if (!picked) return;
    setDiscError(null);
    try {
      setDiscs(naturalSort(await api.listIsos(picked)));
    } catch (e) {
      setDiscError(errorMessage(e));
    }
  };

  /** Runs a disc operation that returns the updated session. */
  const discOp = async (fn: () => Promise<DiscSession | null | void>) => {
    setDiscBusy(true);
    setDiscError(null);
    try {
      const s = await fn();
      if (s) {
        setSession(s);
        setInstaller((prev) => (s.installers.includes(prev) ? prev : s.installers[0] ?? ""));
        // Preselect the game the installer added.
        setExe((prev) => prev ?? s.installed[0] ?? null);
      }
    } catch (e) {
      setDiscError(errorMessage(e));
    } finally {
      setDiscBusy(false);
    }
  };

  /** Looks for what the installer added and moves on to adding the game. */
  const finishInstall = () => {
    setInstallDone(true);
    discOp(api.discSessionInfo);
  };

  useEffect(() => {
    if (wasInstalling.current && !installing) finishInstall();
    wasInstalling.current = installing;
  }, [installing]);

  const install = () => {
    setInstallDone(false);
    onInstall(installer);
  };

  const unmount = () =>
    discOp(async () => {
      await api.endDiscSession();
      setSession(null);
    });

  const waiting = installing && !installDone;
  const ready = kind === "scummvm" ? scummvmInstalled && sources.length > 0 : exe !== null && !waiting;
  const locked = busy || waiting || discBusy;
  const close = locked ? undefined : onClose;

  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <header>
          <h2>Añadir juego</h2>
          <button className="icon-btn" onClick={close} disabled={locked} aria-label="Cerrar"><CloseIcon /></button>
        </header>

        <div className="segmented" role="radiogroup" aria-label="Motor">
          {(["scummvm", "windows"] as ExternalKind[]).map((k) => (
            <button
              key={k}
              role="radio"
              aria-checked={kind === k}
              className={kind === k ? "active" : ""}
              onClick={() => setKind(k)}
              disabled={locked || session !== null}
            >
              {k === "scummvm" ? "ScummVM" : "Windows (Wine/Proton)"}
            </button>
          ))}
        </div>

        {kind === "scummvm" ? (
          <>
            {!scummvmInstalled && (
              <p className="note">ScummVM no está instalado. Instálalo con <code>sudo apt install scummvm</code>.</p>
            )}
            <label>
              Archivos del juego
              <small className="muted">
                Las imágenes ISO (todos los discos) se extraen en la carpeta de la app. Una carpeta se usa tal cual.
              </small>
              <div className="modal-row">
                <button className="ghost-btn" onClick={pickIsos} disabled={busy}>Elegir ISOs…</button>
                <button className="ghost-btn" onClick={pickFolder} disabled={busy}>Elegir carpeta…</button>
              </div>
              {sources.length > 0 && (
                <small className="mono">{sources.map(fileName).join(", ")}</small>
              )}
            </label>
          </>
        ) : (
          <>
            <div className="segmented" role="radiogroup" aria-label="Origen">
              {(["exe", "disc"] as WindowsSource[]).map((s) => (
                <button
                  key={s}
                  role="radio"
                  aria-checked={winSource === s}
                  className={winSource === s ? "active" : ""}
                  onClick={() => setWinSource(s)}
                  disabled={locked || session !== null}
                >
                  {s === "exe" ? "Ya instalado" : "Instalar desde disco"}
                </button>
              ))}
            </div>

            {discMode && !session && (
              <label>
                Discos
                <small className="muted">
                  Las imágenes ISO del juego (una carpeta con todas, o elegidas a mano) o un DVD en el lector.
                  Se montan todas a la vez, cada una como una unidad de CD dentro del prefijo.
                </small>
                <div className="modal-row wrap">
                  <button className="ghost-btn" onClick={pickDiscFolder} disabled={discBusy}>Elegir carpeta…</button>
                  <button className="ghost-btn" onClick={pickDiscIsos} disabled={discBusy}>Elegir ISOs…</button>
                  {drives.map((d) => (
                    <button
                      key={d.device}
                      className="ghost-btn"
                      onClick={() => setDiscs([d.device])}
                      disabled={discBusy || !d.media}
                      title={d.media ? d.device : `${d.device}: sin disco`}
                    >
                      DVD{d.label ? `: ${d.label}` : d.media ? "" : " (vacío)"}
                    </button>
                  ))}
                </div>
                {discs.length > 0 && (
                  <ol className="disc-list mono">
                    {discs.map((d) => <li key={d}>{fileName(d)}</li>)}
                  </ol>
                )}
                <div className="modal-row">
                  <button
                    className="play-btn small"
                    onClick={() => discOp(() => api.startDiscSession(discs))}
                    disabled={discs.length === 0 || discBusy}
                  >
                    {discBusy ? "Montando…" : discs.length > 1 ? `Montar ${discs.length} discos` : "Montar"}
                  </button>
                </div>
              </label>
            )}

            {discMode && session && (
              <>
                <label>
                  Unidades
                  <ol className="disc-list">
                    {session.discs.map((d, i) => (
                      <li key={d.source}>
                        <span className="disc-letter">{d.letter ? `${d.letter}:` : "—"}</span>
                        <span className="disc-name" title={d.root ?? d.source}>
                          {isDevice(d.source) ? "DVD" : fileName(d.source)}
                          {d.label && <small className="muted"> · {d.label}</small>}
                        </span>
                        {isDevice(d.source) && (
                          <>
                            <button className="link-btn" onClick={() => discOp(() => api.changeDisc(i))} disabled={discBusy}>
                              {d.letter ? "Volver a leer" : "Leer"}
                            </button>
                            {d.letter && (
                              <button className="link-btn" onClick={() => discOp(() => api.ejectDisc(i))} disabled={discBusy}>
                                Expulsar
                              </button>
                            )}
                          </>
                        )}
                      </li>
                    ))}
                  </ol>
                </label>

                <label>
                  Instalador
                  {session.installers.length > 0 ? (
                    <div className="modal-row">
                      <select value={installer} onChange={(e) => setInstaller(e.target.value)} disabled={waiting}>
                        {session.installers.map((i) => (
                          <option key={i} value={i}>{session.discs[0].letter}:\{i}</option>
                        ))}
                      </select>
                      <button
                        className="play-btn small"
                        onClick={install}
                        disabled={!installer || waiting || discBusy}
                      >
                        {waiting ? "Instalando…" : installDone ? "Instalar de nuevo" : "Instalar"}
                      </button>
                    </div>
                  ) : (
                    <small className="muted">No hay ningún .exe en la raíz del primer disco.</small>
                  )}
                  <small className="muted">
                    {session.discs.length > 1
                      ? "Cuando pida el siguiente disco, indícale su unidad de la lista. "
                      : "Si pide otro disco, cámbialo en el lector y pulsa «Volver a leer». "}
                    Al terminar, el juego instalado se detecta solo.
                  </small>
                  {waiting && (
                    <div className="modal-row">
                      <button className="ghost-btn" onClick={finishInstall}>La instalación ha terminado</button>
                    </div>
                  )}
                </label>
              </>
            )}

            {discError && <p className="note">{discError}</p>}

            {discMode && session && installDone && (
              <label>
                Juego instalado
                {session.installed.length > 0 ? (
                  <select value={exe ?? ""} onChange={(e) => setExe(e.target.value)}>
                    {exe && !session.installed.includes(exe) && <option value={exe}>{winPath(exe, session.driveC)}</option>}
                    {session.installed.map((p) => (
                      <option key={p} value={p}>{winPath(p, session.driveC)}</option>
                    ))}
                  </select>
                ) : (
                  <small className="muted">No se ha encontrado ningún ejecutable nuevo en C:. Elígelo a mano.</small>
                )}
              </label>
            )}

            {(!discMode || (session && !waiting)) && (
              <label>
                {discMode && installDone ? "Otro ejecutable" : "Ejecutable"}
                <small className="muted">Se lanza en el prefijo de Battle.net con el runner configurado.</small>
                <div className="modal-row">
                  <button className="ghost-btn" onClick={pickExe} disabled={busy || waiting}>Elegir .exe…</button>
                  {discMode && installDone && (
                    <button className="ghost-btn" onClick={() => discOp(api.discSessionInfo)} disabled={discBusy}>Buscar de nuevo</button>
                  )}
                  {session && (
                    <button className="ghost-btn" onClick={unmount} disabled={locked}>{session.discs.length > 1 ? "Desmontar discos" : "Desmontar disco"}</button>
                  )}
                </div>
                {exe && !(discMode && installDone) && <small className="mono">{exe}</small>}
              </label>
            )}
          </>
        )}

        <label>
          Nombre
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={kind === "scummvm" ? "Automático (lo detecta ScummVM)" : "Nombre del ejecutable"}
            disabled={busy}
          />
        </label>

        {busy && <ProgressBar p={progress} />}

        <footer>
          <button className="ghost-btn" onClick={close} disabled={locked}>Cancelar</button>
          <button
            className="play-btn small"
            onClick={() => onAdd({ kind, name, sources: kind === "scummvm" ? sources : [], exe: kind === "windows" ? exe : null })}
            disabled={!ready || locked}
          >
            {busy ? "Añadiendo…" : "Añadir"}
          </button>
        </footer>
      </div>
    </div>
  );
}
