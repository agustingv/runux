import { Fragment, type ReactNode } from "react";
import { type EmulatorInfo, type ExternalKind, type HelpBlock, type HelpSection } from "../api";

interface Props {
  emulators: EmulatorInfo[];
  windowsHelp: HelpSection[];
  /** The page shown: Windows games or an emulator's system. */
  kind: ExternalKind;
  onSelect: (kind: ExternalKind) => void;
  onClose: () => void;
}

/** Text with `code` between backticks. */
function Rich({ text }: { text: string }) {
  return (
    <>
      {text.split("`").map((part, i) => (i % 2 ? <code key={i}>{part}</code> : <Fragment key={i}>{part}</Fragment>))}
    </>
  );
}

function Block({ block }: { block: HelpBlock }) {
  switch (block.type) {
    case "text":
      return <p><Rich text={block.value} /></p>;
    case "steps":
      return <ol>{block.value.map((item, i) => <li key={i}><Rich text={item} /></li>)}</ol>;
    case "list":
      return <ul>{block.value.map((item, i) => <li key={i}><Rich text={item} /></li>)}</ul>;
  }
}

/** What an emulator's page says before its sections. */
function EmulatorIntro({ emulator }: { emulator: EmulatorInfo }) {
  return (
    <>
      <p className="muted">
        {emulator.system === emulator.program ? "Estos juegos" : `Los juegos de ${emulator.system}`} se abren con{" "}
        {emulator.program}. Lo que se configura en él, como los mandos o los gráficos, se hace con el botón «Abrir{" "}
        {emulator.program}» de la página de cualquiera de sus juegos, y vale para todos.
      </p>
      {!emulator.installed && (
        <p className="note">
          {emulator.program} no está instalado.{" "}
          {emulator.installHint ? <>Instálalo con <code>{emulator.installHint}</code>.</> : "Instálalo con el gestor de paquetes de tu distribución."}
        </p>
      )}
    </>
  );
}

/** The help pages: Windows games and each emulator's. */
export function Help({ emulators, windowsHelp, kind, onSelect, onClose }: Props) {
  const pages: { kind: ExternalKind; label: string; intro?: ReactNode; sections: HelpSection[] }[] = [
    { kind: "windows", label: "Windows", sections: windowsHelp },
    ...emulators.map((e) => ({ kind: e.kind, label: e.system, intro: <EmulatorIntro emulator={e} />, sections: e.help })),
  ];
  const page = pages.find((p) => p.kind === kind) ?? pages[0];
  return (
    <div className="help">
      <header className="help-header">
        <h1>Ayuda</h1>
        <button className="ghost-btn" onClick={onClose}>Volver</button>
      </header>

      <nav className="segmented help-pages">
        {pages.map((p) => (
          <button key={p.kind} className={p.kind === page.kind ? "active" : ""} onClick={() => onSelect(p.kind)}>
            {p.label}
          </button>
        ))}
      </nav>

      {page.intro}
      {page.sections.map((section) => (
        <section key={section.title} className="card help-section">
          <h2>{section.title}</h2>
          {section.blocks.map((block, i) => <Block key={i} block={block} />)}
        </section>
      ))}
    </div>
  );
}
