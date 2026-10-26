import { useEffect, useState } from "react";
import { api, type Config, type GeInstall, type Progress, type ProtonBuild, type Runner } from "../api";
import { CloseIcon } from "../icons";

interface Props {
  config: Config;
  onSave: (c: Config) => void;
  onClose: () => void;
  onOpenPrefix: () => void;
  onOpenEaPrefix: () => void;
  onStopWine: () => void;
  /** Downloads the latest GE-Proton; resolves when done, with `undefined` if it failed. */
  onInstallGeProton: () => Promise<GeInstall | undefined>;
  installingGeProton: boolean;
  geProtonProgress?: Progress;
}

const AUTO = "";

export function Settings({
  config, onSave, onClose, onOpenPrefix, onOpenEaPrefix, onStopWine,
  onInstallGeProton, installingGeProton, geProtonProgress,
}: Props) {
  const [draft, setDraft] = useState(config);
  const [protons, setProtons] = useState<ProtonBuild[] | null>(null);

  const loadProtons = () => api.listProtons().then(setProtons).catch(() => setProtons([]));

  useEffect(() => {
    loadProtons();
  }, []);

  const [geResult, setGeResult] = useState<GeInstall | null>(null);
  // Sorted newest first, like every GE-Proton build.
  const downloadedGe = protons?.find((p) => p.downloaded);

  const installGeProton = async () => {
    setGeResult(null);
    const result = await onInstallGeProton();
    if (result) setGeResult(result);
    loadProtons();
  };

  const pct = geProtonProgress?.total
    ? Math.round((geProtonProgress.downloaded / geProtonProgress.total) * 100)
    : null;

  const runnerChanged = draft.runner !== config.runner;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <header>
          <h2>Ajustes</h2>
          <button className="icon-btn" onClick={onClose} aria-label="Cerrar"><CloseIcon /></button>
        </header>

        <div className="segmented" role="radiogroup" aria-label="Runner">
          {(["proton", "wine"] as Runner[]).map((r) => (
            <button
              key={r}
              role="radio"
              aria-checked={draft.runner === r}
              className={draft.runner === r ? "active" : ""}
              onClick={() => setDraft({ ...draft, runner: r })}
            >
              {r === "proton" ? "Proton (recomendado)" : "Wine"}
            </button>
          ))}
        </div>

        {draft.runner === "proton" ? (
          <>
            <label>
              Versión de Proton
              <select
                value={draft.protonPath ?? AUTO}
                onChange={(e) => setDraft({ ...draft, protonPath: e.target.value || null })}
              >
                <option value={AUTO}>
                  Automático{protons?.[0] ? ` (${protons[0].name})` : ""}
                </option>
                {protons?.map((p) => (
                  <option key={p.path} value={p.path}>
                    {p.name}{p.version ? ` — ${p.version}` : ""}
                  </option>
                ))}
              </select>
              <small className="muted">
                {protons === null ? "Buscando versiones de Proton…"
                  : protons.length === 0 ? "No se encontró Proton. Descarga GE-Proton o instala Proton desde Steam."
                  : "Se usan las de Steam (con su Steam Linux Runtime si lo tienes) y las descargadas aquí. Steam no hace falta."}
              </small>
            </label>
            <div className="ge-proton">
              <button className="ghost-btn" onClick={installGeProton} disabled={installingGeProton}>
                {installingGeProton
                  ? pct === null ? "Buscando GE-Proton…" : `Descargando GE-Proton… ${pct}%`
                  : downloadedGe ? "Buscar actualización de GE-Proton" : "Descargar GE-Proton"}
              </button>
              <small className="muted">
                {geResult && !installingGeProton
                  ? geResult.updated
                    ? `${geResult.version} descargado. Elígelo arriba o déjalo en Automático.`
                    : `Ya tienes la última versión (${geResult.version}).`
                  : downloadedGe
                    ? <>Descargado: <b>{downloadedGe.name}</b>. Si sale una versión nueva, se descarga junto a la que tienes.</>
                    : "La última versión, desde GitHub (unos 600 MB). No necesita Steam. Elígela arriba o déjalo en Automático."}
              </small>
            </div>
            <label>
              Datos de Proton
              <input
                value={draft.protonData}
                onChange={(e) => setDraft({ ...draft, protonData: e.target.value })}
              />
              <small className="muted">El prefijo se crea dentro, en <code>pfx/</code>.</small>
            </label>
          </>
        ) : (
          <>
            <label>
              Binario de Wine
              <input
                value={draft.wineBin}
                onChange={(e) => setDraft({ ...draft, wineBin: e.target.value })}
                placeholder="wine o /ruta/a/wine"
              />
              <small className="muted">Nombre en el PATH o ruta absoluta (p. ej. wine-staging).</small>
            </label>
            <label>
              Prefijo
              <input
                value={draft.prefix}
                onChange={(e) => setDraft({ ...draft, prefix: e.target.value })}
              />
            </label>
          </>
        )}

        {runnerChanged && (
          <p className="note">
            Cada runner usa su propio prefijo: al cambiar tendrás que volver a instalar
            Battle.net, la EA app y los juegos en el nuevo.
          </p>
        )}

        <div className="modal-row">
          <button className="ghost-btn" onClick={onOpenPrefix}>Abrir prefijo</button>
          <button className="ghost-btn" onClick={onOpenEaPrefix}>Prefijo de EA</button>
          <button className="ghost-btn danger" onClick={onStopWine}>Detener Wine</button>
        </div>

        <footer>
          <button className="ghost-btn" onClick={onClose}>Cancelar</button>
          <button className="play-btn small" onClick={() => onSave(draft)}>Guardar</button>
        </footer>
      </div>
    </div>
  );
}
