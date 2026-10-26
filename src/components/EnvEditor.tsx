import { useEffect, useState } from "react";
import { errorMessage } from "../api";

const formatEnv = (env: Record<string, string>) =>
  Object.entries(env).map(([k, v]) => `${k}=${v}`).join("\n");

/** `NAME=value` per line; blank lines and `#` comments are skipped. */
function parseEnv(text: string): Record<string, string> {
  const env: Record<string, string> = {};
  text.split("\n").forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const eq = line.indexOf("=");
    if (eq <= 0) throw new Error(`Línea ${i + 1}: falta «NOMBRE=valor»`);
    env[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
  });
  return env;
}

interface Props {
  env: Record<string, string>;
  onSave: (env: Record<string, string>) => Promise<void>;
  /** Replaces the default hint under the text box. */
  hint?: string;
}

export function EnvEditor({ env, onSave, hint }: Props) {
  const [text, setText] = useState(() => formatEnv(env));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const dirty = text.trim() !== formatEnv(env);

  // Follow changes made elsewhere (the troubleshooting switches).
  useEffect(() => setText(formatEnv(env)), [formatEnv(env)]);

  const save = async () => {
    setError(null);
    setSaving(true);
    try {
      const parsed = parseEnv(text);
      await onSave(parsed);
      setText(formatEnv(parsed));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="env-editor">
      <h3 className="section-title">Variables de entorno</h3>
      <textarea
        className="mono"
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder={"WINEDLLOVERRIDES=msvcr70=n\nPROTON_USE_WINED3D=1"}
        spellCheck={false}
        rows={Math.max(3, text.split("\n").length + 1)}
      />
      <div className="env-actions">
        <small className="muted">{hint ?? "Una por línea, NOMBRE=valor. Se aplican la próxima vez que lances el juego."}</small>
        <button className="ghost-btn" onClick={save} disabled={!dirty || saving}>
          {saving ? "Guardando…" : "Guardar"}
        </button>
      </div>
      {error && <p className="note">{error}</p>}
    </div>
  );
}
