import { useEffect, useState } from "react";
import { api, onIsland, onLevels, type Island as IslandState, type Levels } from "./api";
import { stepLabel } from "./format";

/** The floating pill at the top of the screen. Rust decides its state; this only renders it. */
export function Island() {
  const [island, setIsland] = useState<IslandState>({ kind: "hidden" });

  useEffect(() => {
    api.islandState().then(setIsland);
    const unlisten = onIsland(setIsland);
    return () => void unlisten.then((fn) => fn());
  }, []);

  return <div className="island">{render(island)}</div>;
}

function render(island: IslandState) {
  switch (island.kind) {
    case "hidden":
      return null;
    case "detected":
      return (
        <>
          <span className="island-dot idle" />
          <span className="island-text">Llamada en {island.app_name}</span>
          <button className="island-button record" onClick={() => api.startRecording()}>
            grabar
          </button>
          <button className="island-close" onClick={() => api.dismissIsland()} aria-label="Cerrar">
            ×
          </button>
        </>
      );
    case "recording":
      return (
        <>
          <span className="island-dot live" />
          <Elapsed since={island.started_at_ms} />
          <Meters />
          <button className="island-button" onClick={() => api.stopRecording()}>
            detener
          </button>
        </>
      );
    case "processing":
      return (
        <>
          <span className="island-spinner" />
          <span className="island-text">
            {stepLabel(island.step)}
            {island.fraction > 0 && ` ${Math.round(island.fraction * 100)}%`}
          </span>
        </>
      );
    case "done":
      return (
        <>
          <span className={`island-dot ${island.failed ? "failed" : "ok"}`} />
          <span className="island-text">{island.failed ? "No se pudo transcribir" : "Transcripción lista"}</span>
          <button
            className="island-button"
            onClick={() => {
              api.showCall(island.call_id);
              api.dismissIsland();
            }}
          >
            {island.failed ? "Ver" : "Abrir"}
          </button>
        </>
      );
  }
}

function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(id);
  }, []);
  const total = Math.max(0, Math.floor((now - since) / 1000));
  const h = Math.floor(total / 3600);
  const mm = String(Math.floor(total / 60) % 60).padStart(2, "0");
  const ss = String(total % 60).padStart(2, "0");
  return <span className="island-text island-timer">{h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`}</span>;
}

/** Live input levels: you (mic) and them (call app). */
function Meters() {
  const [levels, setLevels] = useState<Levels>({ mic: 0, system: 0 });
  useEffect(() => {
    const unlisten = onLevels(setLevels);
    return () => void unlisten.then((fn) => fn());
  }, []);
  // Speech RMS sits around 0.01-0.2; a square root spreads it over the meter.
  const height = (level: number) => `${Math.min(100, Math.sqrt(level) * 220)}%`;
  return (
    <span className="island-meters" title="Vos / Ellos">
      <span className="island-meter">
        <span style={{ height: height(levels.mic) }} />
      </span>
      <span className="island-meter">
        <span style={{ height: height(levels.system) }} />
      </span>
    </span>
  );
}
