import { useCallback, useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  onIsland,
  onLibraryChanged,
  onPipelineEvent,
  onSelectCall,
  MEDIA_EXTENSIONS,
  type Call,
  type CallId,
  type Doctor,
  type Island,
} from "./api";
import { CallList, type ProgressMap } from "./CallList";
import { CallView } from "./CallView";
import { SetupBanner } from "./SetupBanner";

export function App() {
  const [calls, setCalls] = useState<Call[]>([]);
  const [selected, setSelected] = useState<CallId | null>(null);
  const [progress, setProgress] = useState<ProgressMap>({});
  const [doctor, setDoctor] = useState<Doctor | null>(null);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [island, setIsland] = useState<Island>({ kind: "hidden" });

  const refresh = useCallback(async () => {
    const list = await api.listCalls();
    setCalls(list);
    setSelected((current) => current ?? list[0]?.id ?? null);
  }, []);

  const importPaths = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) return;
      try {
        const imported = await api.importFiles(paths);
        await refresh();
        if (imported[0]) setSelected(imported[0].id);
      } catch (e) {
        setError(String(e));
      }
    },
    [refresh],
  );

  const pickFiles = async () => {
    const paths = await open({ multiple: true, filters: [{ name: "Grabaciones", extensions: MEDIA_EXTENSIONS }] });
    if (paths) await importPaths(paths);
  };

  const toggleRecording = async () => {
    try {
      if (island.kind === "recording") await api.stopRecording();
      else setSelected(await api.startRecording());
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    refresh().catch((e) => setError(String(e)));
    api.doctor().then(setDoctor);
    api.islandState().then(setIsland);
  }, [refresh]);

  // Recording state, new recordings and "open this call" requests from the island.
  useEffect(() => {
    const unlisteners = [
      onIsland(setIsland),
      onLibraryChanged(() => refresh()),
      onSelectCall((id) => refresh().then(() => setSelected(id))),
    ];
    return () => unlisteners.forEach((u) => void u.then((fn) => fn()));
  }, [refresh]);

  // Pipeline progress pushed from Rust.
  useEffect(() => {
    const unlisten = onPipelineEvent((event) => {
      if (event.kind === "step_progress" || event.kind === "step_started") {
        const fraction = event.kind === "step_progress" ? event.fraction : 0;
        setProgress((p) => ({ ...p, [event.call_id]: { step: event.step, fraction } }));
      }
      if (event.kind === "finished" || event.kind === "failed") {
        setProgress(({ [event.call_id]: _, ...rest }) => rest);
      }
      if (event.kind !== "step_progress") refresh();
    });
    return () => void unlisten.then((fn) => fn());
  }, [refresh]);

  // Drag & drop recordings anywhere on the window.
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "enter" || payload.type === "over") setDragging(true);
      else setDragging(false);
      if (payload.type === "drop") importPaths(payload.paths);
    });
    return () => void unlisten.then((fn) => fn());
  }, [importPaths]);

  const current = calls.find((c) => c.id === selected) ?? null;

  return (
    <div className="app">
      <aside className="sidebar">
        <header className="sidebar-header">
          <h1>Llamadas</h1>
          <div className="actions">
            <button className="button" onClick={pickFiles}>
              Importar
            </button>
            <button className="button primary" onClick={toggleRecording} disabled={!doctor?.calls_audio}>
              {island.kind === "recording" ? "■ Detener" : "● Grabar"}
            </button>
          </div>
        </header>
        <CallList calls={calls} selected={selected} progress={progress} onSelect={setSelected} />
      </aside>

      <main className="content">
        {doctor && <SetupBanner doctor={doctor} />}
        {error && (
          <div className="banner error" onClick={() => setError(null)}>
            {error}
          </div>
        )}
        {current ? (
          <CallView call={current} progress={progress[current.id]} />
        ) : (
          <div className="empty">
            <p className="empty-title">Arrastrá una grabación acá</p>
            <p className="muted">Video o audio: OBS, Zoom, Meet, notas de voz…</p>
            <button className="button" onClick={pickFiles}>
              Elegir archivos
            </button>
          </div>
        )}
      </main>

      {dragging && (
        <div className="drop-overlay">
          <p>Soltá para transcribir</p>
        </div>
      )}
    </div>
  );
}
