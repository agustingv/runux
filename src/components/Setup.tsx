import type { ReactNode } from "react";
import { formatBytes, type Progress, type Status } from "../api";
import { CheckIcon } from "../icons";

interface Props {
  status: Status;
  busy: Set<string>;
  progress: Record<string, Progress>;
  onCreatePrefix: () => void;
  onInstallDxvk: () => void;
  onInstallBattlenet: () => void;
  onOpenSettings: () => void;
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
      <h1>Configurar Battle.net en Linux</h1>
      <p className="muted">
        Runux prepara un prefijo {isProton ? "de Proton" : "de Wine"} aislado, instala el cliente oficial de Battle.net y
        después detecta y lanza tus juegos desde aquí.
      </p>

      <ol className="steps">
        <Step
          n={1}
          title={isProton ? "Proton" : "Wine"}
          done={wineOk}
          description={
            isProton
              ? wineOk
                ? `Usando ${status.runnerLabel}. Puedes elegir otra versión en Ajustes.`
                : "No se encontró Proton en tus bibliotecas de Steam. Instálalo desde Steam o cambia a Wine en Ajustes."
              : wineOk ? `Detectado: ${status.runnerLabel} (64 bits)`
              : status.runnerLabel
                ? <>Detectado {status.runnerLabel}, pero sin soporte de 64 bits, que Battle.net necesita.
                    Instala <code>wine64</code> (<code>sudo apt install wine64</code>) o indica otro Wine en Ajustes.</>
                : <>No se encuentra wine. Instálalo (<code>sudo apt install wine wine64</code>) o indica su ruta en Ajustes.</>
          }
        >
          <button className="ghost-btn" onClick={props.onOpenSettings}>Ajustes</button>
        </Step>

        <Step
          n={2}
          title="Prefijo"
          done={status.prefixExists}
          description={
            isProton
              ? "Un entorno Windows dedicado solo a Battle.net. Proton ya incluye DXVK y VKD3D."
              : "Un entorno Windows de 64 bits dedicado solo a Battle.net."
          }
        >
          {!status.prefixExists && (
            <button className="play-btn small" disabled={!wineOk || busy.has("prefix")} onClick={props.onCreatePrefix}>
              {busy.has("prefix") ? "Creando…" : "Crear prefijo"}
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
              <button className="play-btn small" disabled={!status.prefixExists || busy.has("dxvk")} onClick={props.onInstallDxvk}>
                {busy.has("dxvk") ? "Instalando…" : "Instalar DXVK"}
              </button>
              {busy.has("dxvk") && <ProgressBar p={progress.dxvk} />}
            </>
          )}
        </Step>}

        <Step
          n={isProton ? 3 : 4}
          title="Battle.net"
          done={status.battlenetInstalled}
          description={
            busy.has("installer")
              ? "Instalador en marcha: sigue los pasos en su ventana. Este paso se completará solo."
              : "Descarga el instalador oficial de Blizzard y lo ejecuta dentro del prefijo."
          }
        >
          {!status.battlenetInstalled && (
            <>
              <button
                className="play-btn small"
                disabled={!status.prefixExists || busy.has("installer")}
                onClick={props.onInstallBattlenet}
              >
                {busy.has("installer") ? "Instalando…" : "Descargar e instalar"}
              </button>
              {busy.has("installer") && <ProgressBar p={progress.installer} />}
            </>
          )}
        </Step>
      </ol>
    </div>
  );
}
