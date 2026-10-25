import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, onIsland, onLevels, type Island as IslandState } from "./api";
import { formatTimestamp, stepLabel } from "./format";
import { popupMenu, usePersistentState } from "./ui";

type Orientation = "horizontal" | "vertical";

/** The floating pill at the top of the screen. Rust decides its state; this only renders it.
 * It can be dragged anywhere (double click puts it back), folded down to the essentials and
 * stood upright, to sit along a side of the screen. Right click has the options. */
export function Island() {
  const [island, setIsland] = useState<IslandState>({ kind: "hidden" });
  const [folded, setFolded] = usePersistentState("island.folded", false);
  const [orientation, setOrientation] = usePersistentState<Orientation>("island.orientation", "horizontal");
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.islandState().then(setIsland);
    const unlisten = onIsland(setIsland);
    return () => void unlisten.then((fn) => fn());
  }, []);

  // The window is exactly the island: transparent margins would still catch clicks.
  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      const { width, height } = el.getBoundingClientRect();
      if (width && height) void api.islandResize(Math.ceil(width), Math.ceil(height));
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const drag = (e: MouseEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    if (e.detail === 2) void api.islandResetPlace();
    else void getCurrentWindow().startDragging();
  };

  // Only what runs for a while folds; asking and telling need their buttons.
  const foldable = island.kind === "recording" || island.kind === "processing";
  const fold = foldable && (
    <button
      className="island-icon"
      onClick={() => setFolded(!folded)}
      aria-label={folded ? "Expandir" : "Achicar"}
      title={folded ? "Expandir" : "Achicar"}
    >
      {folded ? <ExpandGlyph /> : <FoldGlyph />}
    </button>
  );

  const menu = (e: MouseEvent) => {
    e.preventDefault();
    void popupMenu([
      { text: "Horizontal", checked: orientation === "horizontal", action: () => setOrientation("horizontal") },
      { text: "Vertical", checked: orientation === "vertical", action: () => setOrientation("vertical") },
      "separator",
      { text: folded ? "Expandir" : "Achicar", enabled: foldable, action: () => setFolded(!folded) },
      { text: "Volver arriba al centro", action: () => void api.islandResetPlace() },
    ]);
  };

  return (
    <div
      ref={root}
      className={`island ${orientation} ${foldable && folded ? "folded" : ""}`}
      data-kind={island.kind}
      onMouseDown={drag}
      onContextMenu={menu}
      title={island.kind === "hidden" ? undefined : "Arrastrá para moverla · clic derecho: opciones"}
    >
      {render(island, foldable && folded, orientation)}
      {fold}
    </div>
  );
}

function render(island: IslandState, folded: boolean, orientation: Orientation) {
  switch (island.kind) {
    case "hidden":
      return null;
    case "detected":
      return (
        <>
          <span className="island-mark idle" />
          <span className="island-label">
            <span className="island-faint">llamada en</span> {island.app_name}
          </span>
          <button className="island-button primary" onClick={() => api.startRecording()}>
            grabar
          </button>
          <button className="island-icon" onClick={() => api.dismissIsland()} aria-label="Cerrar" title="Cerrar">
            <CloseGlyph />
          </button>
        </>
      );
    case "recording":
      return (
        <>
          <span className="island-mark live" />
          <Elapsed since={island.started_at_ms} />
          {!folded && (
            <>
              {island.app_name && <span className="island-label island-faint">{island.app_name}</span>}
              <Wave columns={orientation === "vertical" ? 24 : HISTORY} />
              <MarkButton />
              <button className="island-button" onClick={() => api.stopRecording()}>
                detener
              </button>
            </>
          )}
        </>
      );
    case "processing": {
      const percent = Math.round(island.fraction * 100);
      return (
        <>
          <span className="island-spinner" />
          {!folded && <span className="island-label">{stepLabel(island.step)}</span>}
          <span className="island-progress">
            <span style={{ width: `${percent}%` }} />
          </span>
          <span className="island-label island-percent">{percent}%</span>
        </>
      );
    }
    case "done":
      return (
        <>
          <span className={`island-mark ${island.failed ? "failed" : "ok"}`} />
          <span
            className="island-label"
            title={island.postponed ? "No había memoria libre para el resumen: generalo desde la llamada" : undefined}
          >
            {island.failed ? "no se pudo procesar" : "lista"}
            {island.postponed && <span className="island-faint"> · sin resumen</span>}
            {island.title && <span className="island-title">{island.title}</span>}
          </span>
          <button
            className="island-button primary"
            onClick={() => {
              api.showCall(island.call_id);
              api.dismissIsland();
            }}
          >
            {island.failed ? "ver" : "abrir"}
          </button>
          <button className="island-icon" onClick={() => api.dismissIsland()} aria-label="Cerrar" title="Cerrar">
            <CloseGlyph />
          </button>
        </>
      );
  }
}

/** Marks this moment of the call; it shows in the call's notes and transcript. */
function MarkButton() {
  const [marked, setMarked] = useState(false);
  useEffect(() => {
    if (!marked) return;
    const timer = setTimeout(() => setMarked(false), 1200);
    return () => clearTimeout(timer);
  }, [marked]);
  return (
    <button
      className="island-button"
      title="Marcar este momento: queda en las notas de la llamada"
      onClick={() => api.addMark().then(() => setMarked(true), () => {})}
    >
      {marked ? "marcado" : "marcar"}
    </button>
  );
}

function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(id);
  }, []);
  return <span className="island-timer">{formatTimestamp(Math.max(0, now - since))}</span>;
}

/** Levels come ten times a second: four seconds of them. */
const HISTORY = 40;
const silence = () => Array<number>(HISTORY).fill(0);

/** The last seconds of sound, as columns of dots: you above the line, them below (upright:
 * you left, them right, newest at the bottom). */
function Wave({ columns }: { columns: number }) {
  const [levels, setLevels] = useState({ mic: silence(), system: silence() });
  useEffect(() => {
    const unlisten = onLevels(({ mic, system }) =>
      setLevels((l) => ({ mic: [...l.mic.slice(1), mic], system: [...l.system.slice(1), system] })),
    );
    return () => void unlisten.then((fn) => fn());
  }, []);
  return (
    <span className="island-wave" title="Arriba vos, abajo ellos">
      <span className="island-wave-half mic">
        {levels.mic.slice(-columns).map((level, i) => (
          <span key={i} style={dots(level)} />
        ))}
      </span>
      <span className="island-wave-half system">
        {levels.system.slice(-columns).map((level, i) => (
          <span key={i} style={dots(level)} />
        ))}
      </span>
    </span>
  );
}

/** A level in whole dots, at least one; the CSS makes them 2px plus a 1px gap each. Speech
 * RMS sits around 0.01-0.2; a square root spreads it over the four dots there are. */
function dots(level: number) {
  return { "--dots": Math.max(1, Math.min(4, Math.round(Math.sqrt(level) * 9))) } as CSSProperties;
}

function FoldGlyph() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
      <path d="M1 5h8" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}

function ExpandGlyph() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
      <path d="M1 5h8M5 1v8" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}

function CloseGlyph() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
      <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}
