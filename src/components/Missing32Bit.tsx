import { useState } from "react";
import type { Status } from "../api";

/** Warns that 32-bit graphics drivers are missing, with the commands to install them. */
export function Missing32Bit({ missing }: { missing: NonNullable<Status["missing32bit"]> }) {
  const [copied, setCopied] = useState(false);

  const copy = () =>
    navigator.clipboard.writeText(missing.command ?? "").then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    });

  return (
    <div className="note missing-32bit">
      <p>
        <b>Faltan los controladores gráficos de 32 bits.</b> Battle.net, la EA app y muchos juegos son programas de
        32 bits y se cerrarán al abrirse sin ellos.{" "}
        {missing.command ? "Instálalos con:" : "Instala la versión de 32 bits de Mesa (o del controlador de NVIDIA) con el gestor de paquetes de tu distribución."}
      </p>
      {missing.command && (
        <div className="missing-32bit-cmd">
          <pre>{missing.command}</pre>
          <button className="ghost-btn" onClick={copy}>{copied ? "Copiado" : "Copiar"}</button>
        </div>
      )}
      <p>Después, reinicia Runux.</p>
    </div>
  );
}
