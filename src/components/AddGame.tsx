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

/** Steps of installing from a disc. */
const DISC_STEPS = ["Discos", "Instalar", "Añadir a la biblioteca"];

const fileName = (p: string) => p.split("/").pop() ?? p;
/** Folder holding `path`, which usually names the game (`…/CAPCOM/resident evil 4/game.exe`). */
const folderName = (p: string) => p.split("/").slice(-2, -1)[0] ?? "";
const isDevice = (disc: string) => disc.startsWith("/dev/");
/** `C:\…` form of a path inside the prefix's `drive_c`. */
const winPath = (path: string, driveC: string) =>
  path.startsWith(driveC + "/") ? "C:" + path.slice(driveC.length).split("/").join("\\") : path;

function Steps({ current }: { current: number }) {
  return (
    <ol className="steps">
      {DISC_STEPS.map((s, i) => (
        <li key={s} className={i === current ? "current" : i < current ? "done" : ""}>
          <span className="step-num">{i < current ? "✓" : i + 1}</span>
          {s}
        </li>
      ))}
    </ol>
  );
}

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
  const step = !session ? 0 : installDone ? 2 : 1;

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

  // Some installers (InstallShield) exit right after starting the real one,
  // so keep looking until the game shows up.
  const nothingFound = discMode && installDone && session !== null && session.installed.length === 0;
  useEffect(() => {
    if (!nothingFound) return;
    const timer = setInterval(async () => {
      const s = await api.discSessionInfo().catch(() => null);
      if (s && s.installed.length > 0) {
        setSession(s);
        setExe((prev) => prev ?? s.installed[0]);
      }
    }, 3000);
    return () => clearInterval(timer);
  }, [nothingFound]);

  const install = () => {
    setInstallDone(false);
    onInstall(installer);
  };

  const unmount = () =>
    discOp(async () => {
      await api.endDiscSession();
      setSession(null);
      setInstallDone(false);
      setExe(null);
    });

  const waiting = installing && !installDone;
  const locked = busy || waiting || discBusy;
  const close = locked ? undefined : onClose;
  const ready = kind === "scummvm" ? scummvmInstalled && sources.length > 0 : exe !== null && !waiting;
  // `game.exe` says little; the folder it was installed to names the game.
  const suggestedName = discMode && exe ? folderName(exe) : "";

  const add = () =>
    onAdd({
      kind,
      name: name.trim() || suggestedName,
      sources: kind === "scummvm" ? sources : [],
      exe: kind === "windows" ? exe : null,
    });

  const nameField = (
    <label>
      Nombre
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder={suggestedName || (kind === "scummvm" ? "Automático (lo detecta ScummVM)" : "Nombre del ejecutable")}
        disabled={busy}
      />
    </label>
  );

  /** The executables the installer added, to pick the game from. */
  const candidates = session ? [...session.installed] : [];
  if (exe && !candidates.includes(exe)) candidates.unshift(exe);

  let body: React.ReactNode;
  let primary: React.ReactNode;

  if (kind === "scummvm") {
    body = (
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
          {sources.length > 0 && <small className="mono">{sources.map(fileName).join(", ")}</small>}
        </label>
        {nameField}
      </>
    );
  } else if (!discMode) {
    body = (
      <>
        <label>
          Ejecutable
          <small className="muted">El .exe de un juego ya instalado. Se lanza en el prefijo de Battle.net con el runner configurado.</small>
          <div className="modal-row">
            <button className="ghost-btn" onClick={pickExe} disabled={busy}>Elegir .exe…</button>
          </div>
          {exe && <small className="mono">{exe}</small>}
        </label>
        {nameField}
      </>
    );
  } else if (step === 0) {
    body = (
      <label>
        ¿Dónde está el juego?
        <small className="muted">
          Las imágenes ISO del juego (una carpeta con todas, o elegidas a mano) o el DVD del lector.
          Se montan a la vez, cada una como una unidad de CD dentro del prefijo.
        </small>
        <div className="modal-row wrap">
          <button className="ghost-btn" onClick={pickDiscFolder} disabled={discBusy}>Elegir carpeta…</button>
          <button className="ghost-btn" onClick={pickDiscIsos} disabled={discBusy}>Elegir ISOs…</button>
          {drives.map((d) => (
            <button
              key={d.device}
              className={discs[0] === d.device ? "ghost-btn active" : "ghost-btn"}
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
            {discs.map((d) => <li key={d}>{isDevice(d) ? "DVD" : fileName(d)}</li>)}
          </ol>
        )}
      </label>
    );
    primary = (
      <button
        className="play-btn small"
        onClick={() => discOp(() => api.startDiscSession(discs))}
        disabled={discs.length === 0 || discBusy}
      >
        {discBusy ? "Montando…" : discs.length > 1 ? `Montar ${discs.length} discos` : "Montar"}
      </button>
    );
  } else if (step === 1 && session) {
    body = (
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

        {waiting ? (
          <div className="step-box">
            <strong>Instalando…</strong>
            <span>Sigue los pasos en la ventana del instalador.</span>
            <small className="muted">
              {session.discs.length > 1
                ? "Cuando pida el siguiente disco, indícale su unidad de la lista de arriba."
                : "Si pide otro disco, cámbialo en el lector y pulsa «Volver a leer»."}
            </small>
            <small className="muted">
              Al terminar, vuelve aquí para añadir el juego a la biblioteca.{" "}
              <button className="link-btn" onClick={finishInstall}>La instalación ya ha terminado</button>
            </small>
          </div>
        ) : (
          <label>
            Instalador
            {session.installers.length > 0 ? (
              <select value={installer} onChange={(e) => setInstaller(e.target.value)}>
                {session.installers.map((i) => (
                  <option key={i} value={i}>{session.discs[0].letter}:\{i}</option>
                ))}
              </select>
            ) : (
              <small className="muted">No hay ningún .exe en la raíz del primer disco.</small>
            )}
            <small className="muted">
              Instala el juego como en Windows. Al terminar, Runux busca lo que se ha instalado.{" "}
              <button className="link-btn" onClick={finishInstall} disabled={discBusy}>¿Ya está instalado? Saltar este paso</button>
            </small>
          </label>
        )}
      </>
    );
    primary = (
      <button className="play-btn small" onClick={install} disabled={!installer || waiting || discBusy}>
        {waiting ? "Instalando…" : "Instalar"}
      </button>
    );
  } else if (session) {
    body = (
      <>
        <label>
          ¿Cuál es el ejecutable del juego?
          <small className="muted">El que se abrirá al pulsar «Jugar».</small>
          {candidates.length > 0 ? (
            <ul className="disc-list choice-list">
              {candidates.map((p, i) => (
                <li key={p} className={p === exe ? "selected" : ""} onClick={() => setExe(p)}>
                  <input type="radio" name="exe" checked={p === exe} onChange={() => setExe(p)} />
                  <span className="disc-name mono" title={winPath(p, session.driveC)}>{winPath(p, session.driveC)}</span>
                  {i === 0 && session.installed[0] === p && <small className="muted">recomendado</small>}
                </li>
              ))}
            </ul>
          ) : (
            <div className="step-box">
              <strong>Buscando el juego en C:…</strong>
              <small className="muted">
                Si el instalador sigue abierto, termínalo y aparecerá aquí. Si no aparece, elígelo a mano.
              </small>
            </div>
          )}
          <div className="modal-row wrap">
            <button className="ghost-btn" onClick={pickExe} disabled={busy}>Elegir otro .exe…</button>
            <button className="link-btn" onClick={() => setInstallDone(false)} disabled={busy}>Volver a instalar</button>
          </div>
        </label>
        {nameField}
      </>
    );
  }

  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <header>
          <h2>Añadir juego</h2>
          <button className="icon-btn" onClick={close} disabled={locked} aria-label="Cerrar"><CloseIcon /></button>
        </header>

        {session ? (
          <Steps current={step} />
        ) : (
          <>
            <div className="segmented" role="radiogroup" aria-label="Motor">
              {(["scummvm", "windows"] as ExternalKind[]).map((k) => (
                <button
                  key={k}
                  role="radio"
                  aria-checked={kind === k}
                  className={kind === k ? "active" : ""}
                  onClick={() => setKind(k)}
                  disabled={locked}
                >
                  {k === "scummvm" ? "ScummVM" : "Windows (Wine/Proton)"}
                </button>
              ))}
            </div>
            {kind === "windows" && (
              <div className="segmented" role="radiogroup" aria-label="Origen">
                {(["exe", "disc"] as WindowsSource[]).map((s) => (
                  <button
                    key={s}
                    role="radio"
                    aria-checked={winSource === s}
                    className={winSource === s ? "active" : ""}
                    onClick={() => setWinSource(s)}
                    disabled={locked}
                  >
                    {s === "exe" ? "Ya instalado" : "Instalar desde disco"}
                  </button>
                ))}
              </div>
            )}
            {discMode && <Steps current={0} />}
          </>
        )}

        {body}

        {discError && <p className="note">{discError}</p>}
        {busy && <ProgressBar p={progress} />}

        <footer>
          {session && (
            <button className="ghost-btn footer-start" onClick={unmount} disabled={locked}>
              {session.discs.length > 1 ? "Desmontar discos" : "Desmontar disco"}
            </button>
          )}
          <button className="ghost-btn" onClick={close} disabled={locked}>Cancelar</button>
          {primary ?? (
            <button className="play-btn small" onClick={add} disabled={!ready || locked}>
              {busy ? "Añadiendo…" : discMode ? "Añadir a la biblioteca" : "Añadir"}
            </button>
          )}
        </footer>
      </div>
    </div>
  );
}
