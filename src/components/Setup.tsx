import type { ReactNode } from "react";
import { formatBytes, TASK, type Progress, type Status } from "../api";
import { CheckIcon } from "../icons";
import { Missing32Bit } from "./Missing32Bit";

interface Props {
  status: Status;
  busy: Set<string>;
  progress: Record<string, Progress>;
  onCreatePrefix: () => void;
  onInstallDxvk: () => void;
  onInstallBattlenet: () => void;
  onInstallEa: () => void;
  onOpenSettings: () => void;
  /** Downloads GE-Proton and the runtime it needs. */
  onInstallGeProton: () => void;
}

interface StepProps {
  n: number;
  title: string;
  done: boolean;
  description: ReactNode;
  children?: ReactNode;
}

function Step({ n, title, done, description, children }: StepProps) {
  return (
    <li className={`step ${done ? "done" : ""}`}>
      <div className="step-num">{done ? <CheckIcon size={16} /> : n}</div>
      <div className="step-body">
        <h3>{title}</h3>
        <div className="muted">{description}</div>
        {children && <div className="step-actions">{children}</div>}
      </div>
    </li>
  );
}

export function ProgressBar({ p }: { p?: Progress }) {
  if (!p) return null;
  const pct = p.total ? (p.downloaded / p.total) * 100 : null;
  return (
    <div className="progress">
      <div className="progress-fill" style={{ width: pct === null ? "100%" : `${pct}%` }} />
      <span>
        {formatBytes(p.downloaded)}
        {p.total ? ` / ${formatBytes(p.total)}` : ""}
      </span>
    </div>
  );
}

export function Setup(props: Props) {
  const { status, busy, progress } = props;
  const wineOk = status.runnerOk;
  const isProton = status.runner === "proton";

  return (
    <div className="setup">
      <h1>Configurar Runux</h1>
      <p className="muted">
        Runux prepara prefijos {isProton ? "de Proton" : "de Wine"} aislados, instala los clientes oficiales de
        Battle.net y la EA app y después detecta y lanza tus juegos desde aquí.
      </p>
      {status.missing32bit && <Missing32Bit missing={status.missing32bit} />}

      <ol className="steps">
        <Step
          n={1}
          title={isProton ? "Proton" : "Wine"}
          done={wineOk}
          description={
            isProton
              ? wineOk
                ? `Usando ${status.runnerLabel}. Puedes elegir otra versión en Ajustes.`
                : busy.has(TASK.geProton)
                  ? "Descargando GE-Proton y el Steam Linux Runtime en el que se ejecuta. Puede tardar unos minutos."
                  : <>{status.runnerProblem ?? "No se encontró Proton."} No necesitas Steam: Runux
                      descarga GE-Proton y lo que necesita (unos 800 MB en total).</>
              : wineOk ? `Detectado: ${status.runnerLabel} (64 bits)`
              : status.runnerLabel
                ? <>Detectado {status.runnerLabel}, pero sin soporte de 64 bits, que Battle.net necesita.
                    Instala <code>wine64</code> (<code>sudo apt install wine64</code>) o indica otro Wine en Ajustes.</>
                : <>No se encuentra wine. Instálalo (<code>sudo apt install wine wine64</code>) o indica su ruta en Ajustes.</>
          }
        >
          {isProton && !wineOk && (
            <>
              <button className="play-btn small" disabled={busy.has(TASK.geProton)} onClick={props.onInstallGeProton}>
                {busy.has(TASK.geProton) ? "Descargando…" : "Descargar GE-Proton"}
              </button>
              {busy.has(TASK.geProton) && <ProgressBar p={progress[TASK.geProton]} />}
            </>
          )}
          <button className="ghost-btn" onClick={props.onOpenSettings}>Ajustes</button>
        </Step>

        <Step
          n={2}
          title="Prefijo"
          done={status.prefixExists}
          description={
            isProton
              ? "Un entorno Windows dedicado a Battle.net. Proton ya incluye DXVK y VKD3D. La EA app crea el suyo al instalarla."
              : "Un entorno Windows de 64 bits dedicado a Battle.net. La EA app crea el suyo al instalarla."
          }
        >
          {!status.prefixExists && (
            <button className="play-btn small" disabled={!wineOk || busy.has(TASK.prefix)} onClick={props.onCreatePrefix}>
              {busy.has(TASK.prefix) ? "Creando…" : "Crear prefijo"}
            </button>
          )}
        </Step>

        {!isProton && <Step
          n={3}
          title="DXVK (recomendado)"
          done={status.dxvkVersion !== null}
          description={
            status.dxvkVersion ? `Instalado: ${status.dxvkVersion}`
              : "Traduce DirectX 9/10/11 a Vulkan. Mejora mucho el rendimiento y evita fallos gráficos."
          }
        >
          {status.dxvkVersion === null && (
            <>
              <button className="play-btn small" disabled={!status.prefixExists || busy.has(TASK.dxvk)} onClick={props.onInstallDxvk}>
                {busy.has(TASK.dxvk) ? "Instalando…" : "Instalar DXVK"}
              </button>
              {busy.has(TASK.dxvk) && <ProgressBar p={progress[TASK.dxvk]} />}
            </>
          )}
        </Step>}

        <Step
          n={isProton ? 3 : 4}
          title="Battle.net y EA app"
          done={status.battlenetInstalled || status.eaInstalled}
          description="Instala al menos uno de los dos clientes. El otro puedes añadirlo después desde la pantalla de inicio."
        >
          <div className="store-options">
            <div className="store-option">
              <div className="store-text">
                <b>Battle.net</b>
                <span className="muted">
                  {busy.has(TASK.installer)
                    ? "Instalador en marcha: sigue los pasos en su ventana. Este paso se completará solo."
                    : status.prefixExists
                      ? "Descarga el instalador oficial de Blizzard y lo ejecuta en su prefijo."
                      : "Crea primero el prefijo del paso 2."}
                </span>
                {busy.has(TASK.installer) && <ProgressBar p={progress[TASK.installer]} />}
              </div>
              <button
                className="play-btn small"
                disabled={!status.prefixExists || busy.has(TASK.installer)}
                onClick={props.onInstallBattlenet}
              >
                {busy.has(TASK.installer) ? "Instalando…" : "Instalar"}
              </button>
            </div>
            <div className="store-option">
              <div className="store-text">
                <b>EA app</b>
                <span className="muted">
                  {busy.has(TASK.eaInstaller)
                    ? "Instalador en marcha: sigue los pasos en su ventana. Este paso se completará solo."
                    : "Descarga el instalador oficial de Electronic Arts y lo ejecuta en un prefijo propio."}
                </span>
                {busy.has(TASK.eaInstaller) && <ProgressBar p={progress[TASK.eaInstaller]} />}
              </div>
              <button
                className="play-btn small"
                disabled={!wineOk || busy.has(TASK.eaInstaller)}
                onClick={props.onInstallEa}
              >
                {busy.has(TASK.eaInstaller) ? "Instalando…" : "Instalar"}
              </button>
            </div>
          </div>
        </Step>
      </ol>
    </div>
  );
}
