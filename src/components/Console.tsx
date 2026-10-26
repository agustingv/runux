import { useEffect, useRef } from "react";
import type { LogLine } from "../api";

export function Console({ logs, onClear }: { logs: LogLine[]; onClear: () => void }) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => end.current?.scrollIntoView({ block: "end" }), [logs]);

  return (
    <div className="console">
      <div className="console-head">
        <span>Registro</span>
        <button className="link-btn" onClick={onClear}>Limpiar</button>
      </div>
      <div className="console-body">
        {logs.length === 0 && <div className="muted">Sin mensajes.</div>}
        {logs.map((l, i) => (
          <div key={i} className="log-line">
            <span className="log-task">[{l.task}]</span> {l.line}
          </div>
        ))}
        <div ref={end} />
      </div>
    </div>
  );
}
