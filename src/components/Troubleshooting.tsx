import { useState, type ReactNode } from "react";
import { api, errorMessage, type EmulatorFix, type EmulatorInfo, type Runner } from "../api";

/** A fix that is just an environment variable with a given value. */
interface EnvFix {
  symptom: string;
  explanation: ReactNode;
  name: string;
  value: string;
}

/** Fixes for problems seen with old Windows games under Wine/Proton. */
function envFixes(runner: Runner, openLogs: () => void): EnvFix[] {
  const fixes: EnvFix[] = [
    {
      symptom: "El ratón o los clics no responden en los menús",
      explanation:
        "Dibuja con WineD3D en lugar de DXVK. Va algo más lento, pero arregla el cursor y los clics en " +
        "muchos juegos DirectX 9 antiguos (por ejemplo, Mass Effect 3).",
      ...(runner === "proton"
        ? { name: "PROTON_USE_WINED3D", value: "1" }
        : { name: "WINEDLLOVERRIDES", value: "d3d9,d3d10core,d3d11,dxgi=b" }),
    },
    {
      symptom: "Se cierra al arrancar o va a tirones en procesadores con muchos núcleos",
      explanation: "Algunos juegos antiguos fallan si ven demasiados núcleos. Les muestra solo 4.",
      name: "WINE_CPU_TOPOLOGY",
      value: "4:0,1,2,3",
    },
  ];
  if (runner === "proton") {
    fixes.push({
      symptom: "No arranca y no sabes por qué",
      explanation: (
        <>Guarda un registro detallado (<code>steam-0.log</code>, el nombre lo pone Proton) que ayuda a
          encontrar el fallo. Desactívalo después: el archivo crece mucho.{" "}
          <button className="link-btn" onClick={openLogs}>Abrir la carpeta del registro</button></>
      ),
      name: "PROTON_LOG",
      value: "1",
    });
  }
  return fixes;
}

interface Props {
  runner: Runner;
  env: Record<string, string>;
  /** The game's task (`ea:<id>`, `ext:<id>`), which names its log folder. */
  logKey: string;
  onSaveEnv: (env: Record<string, string>) => Promise<void>;
  /** The EA app's in-game overlay, for EA games; `null` before signing in. */
  eaOverlay?: { enabled: boolean | null; onSet: (enabled: boolean) => Promise<void> };
}

function Fix({ symptom, explanation, on, onToggle }: {
  symptom: string;
  explanation: ReactNode;
  on: boolean;
  onToggle?: () => void;
}) {
  return (
    <li className="fix">
      <div>
        <div className="fix-symptom">{symptom}</div>
        <div className="muted">{explanation}</div>
      </div>
      {onToggle && (
        <button
          role="switch"
          aria-checked={on}
          aria-label={symptom}
          className={`switch ${on ? "on" : ""}`}
          onClick={onToggle}
        />
      )}
    </li>
  );
}

/** Known problems by symptom, each with a switch that applies its fix. */
export function Troubleshooting({ runner, env, logKey, onSaveEnv, eaOverlay }: Props) {
  const [error, setError] = useState<string | null>(null);

  const run = (fn: () => Promise<void>) => {
    setError(null);
    fn().catch((e) => setError(errorMessage(e)));
  };

  const toggleEnv = (fix: EnvFix) =>
    run(() => {
      const next = { ...env };
      if (next[fix.name] === fix.value) delete next[fix.name];
      else next[fix.name] = fix.value;
      return onSaveEnv(next);
    });

  return (
    <div className="troubleshooting">
      <h3 className="section-title">Solución de problemas</h3>
      <ul className="fixes">
        {envFixes(runner, () => run(() => api.openGameLogs(logKey))).map((fix) => (
          <Fix
            key={fix.name}
            symptom={fix.symptom}
            explanation={<>{fix.explanation} <span className="mono">{fix.name}={fix.value}</span></>}
            on={env[fix.name] === fix.value}
            onToggle={() => toggleEnv(fix)}
          />
        ))}
        {eaOverlay && (
          <Fix
            symptom="Fallan teclas, alt+tab o el ratón dentro del juego"
            explanation={
              eaOverlay.enabled === null
                ? "La superposición de la EA app captura la entrada del juego. Abre la EA app e inicia sesión para poder desactivarla."
                : "Desactiva la superposición de la EA app, que se inyecta en los juegos y con Wine/Proton puede " +
                  "quedarse con su teclado y ratón. Afecta a todos los juegos de EA."
            }
            on={eaOverlay.enabled === false}
            onToggle={eaOverlay.enabled === null ? undefined : () => run(() => eaOverlay.onSet(!eaOverlay.enabled))}
          />
        )}
        <Fix
          symptom="La resolución es muy baja o muy alta"
          explanation={
            <>Cámbiala en las opciones gráficas del juego. Si no las tiene, busca en su carpeta un configurador
              (un <code>.exe</code> con «Config» o «Launcher» en el nombre). En pantallas de alta densidad,
              la mitad de la resolución nativa se ve nítida y deja los textos legibles.</>
          }
          on={false}
        />
        <Fix
          symptom="El juego no reconoce el mando"
          explanation={
            <>Muchos juegos antiguos de PC solo admiten teclado y ratón (por ejemplo, el Mass Effect 3 original).
              Usa AntiMicroX para
              convertir los botones y palancas del mando en teclas y movimientos del ratón; instálalo con{" "}
              <code>flatpak install flathub io.github.antimicrox.antimicrox</code>. Si el juego sí tiene soporte de
              mando y no lo detecta, comprueba que el sistema lo ve y prueba a conectarlo antes de lanzarlo.</>
          }
          on={false}
        />
      </ul>
      {error && <p className="note">{error}</p>}
      <p className="muted small">
        Los interruptores añaden o quitan variables en el editor de abajo. Se aplican la próxima vez que lances el juego.
      </p>
    </div>
  );
}

/** An emulator's known problems, each with a switch that sets, for this game, the setting fixing it. */
export function EmulatorTroubleshooting({ emulator, settings, onSave }: {
  emulator: EmulatorInfo;
  settings: Record<string, string>;
  onSave: (settings: Record<string, string>) => Promise<void>;
}) {
  const [error, setError] = useState<string | null>(null);

  const on = (fix: EmulatorFix) => settings[fix.key] === fix.value;

  const toggle = (fix: EmulatorFix) => {
    setError(null);
    const next = { ...settings };
    if (on(fix)) delete next[fix.key];
    else next[fix.key] = fix.value;
    onSave(next).catch((e) => setError(errorMessage(e)));
  };

  return (
    <div className="troubleshooting">
      <h3 className="section-title">Solución de problemas</h3>
      <ul className="fixes">
        {emulator.fixes.map((fix) => (
          <Fix
            key={fix.key}
            symptom={fix.symptom}
            explanation={<>{fix.explanation} <span className="mono">{fix.value ? `${fix.key}=${fix.value}` : fix.key}</span></>}
            on={on(fix)}
            onToggle={() => toggle(fix)}
          />
        ))}
      </ul>
      {error && <p className="note">{error}</p>}
      <p className="muted small">
        Se aplican la próxima vez que lances el juego, solo a este juego, y no cambian la configuración de {emulator.program}.
      </p>
    </div>
  );
}
