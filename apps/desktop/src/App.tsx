import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  api,
  onIsland,
  onLibraryChanged,
  onPipelineEvent,
  onSelectCall,
  onSettingsChanged,
  MEDIA_EXTENSIONS,
  type Call,
  type CallId,
  type Doctor,
  type Folder,
  type Island,
} from "./api";
import { CallList, SORT_LABELS, sortCalls, type ProgressMap, type Sort } from "./CallList";
import { CallView } from "./CallView";
import { useCallDrag } from "./drag";
import { Folders, inScope, type Scope } from "./Folders";
import { SearchIcon, SlidersIcon, SortIcon } from "./icons";
import { SetupBanner } from "./SetupBanner";
import { plural } from "./SummaryView";
import { normalize, popupMenu, usePersistentState } from "./ui";

export function App() {
  const [calls, setCalls] = useState<Call[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [selected, setSelected] = useState<CallId | null>(null);
  const [progress, setProgress] = useState<ProgressMap>({});
  const [doctor, setDoctor] = useState<Doctor | null>(null);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [island, setIsland] = useState<Island>({ kind: "hidden" });
  const [scope, setScope] = usePersistentState<Scope>("sidebar.scope", { kind: "all" });
  const [sort, setSort] = usePersistentState<Sort>("sidebar.sort", "newest");
  const [query, setQuery] = useState("");
  const [renaming, setRenaming] = useState<CallId | null>(null);
  const [loaded, setLoaded] = useState(false);

  const showError = useCallback((e: unknown) => setError(String(e)), []);

  const refresh = useCallback(async () => {
    const [list, folderList] = await Promise.all([api.listCalls(), api.listFolders()]);
    setCalls(list);
    setFolders(folderList);
    setLoaded(true);
    setSelected((current) => current ?? list[0]?.id ?? null);
  }, []);

  // A remembered folder that no longer exists falls back to every call.
  useEffect(() => {
    if (loaded && scope.kind === "folder" && !folders.some((f) => f.name === scope.name)) setScope({ kind: "all" });
  }, [loaded, folders, scope, setScope]);

  const moveCall = useCallback(
    async (call: Call, folder: string | null) => {
      try {
        await api.moveCall(call.id, folder);
        await refresh();
      } catch (e) {
        showError(e);
      }
    },
    [refresh, showError],
  );
  const { drag, start: startDrag, justDropped } = useCallDrag(moveCall);

  const importPaths = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) return;
      try {
        const imported = await api.importFiles(paths);
        // Imported while looking at a folder: that is probably where they belong.
        if (scope.kind === "folder") {
          await Promise.all(imported.filter((c) => !c.folder).map((c) => api.moveCall(c.id, scope.name)));
        }
        await refresh();
        if (imported[0]) setSelected(imported[0].id);
      } catch (e) {
        showError(e);
      }
    },
    [refresh, scope, showError],
  );

  const pickFiles = async () => {
    const paths = await open({ multiple: true, filters: [{ name: "Grabaciones", extensions: MEDIA_EXTENSIONS }] });
    if (paths) await importPaths(paths);
  };

  const toggleRecording = async () => {
    try {
      if (recording) await api.stopRecording();
      else setSelected(await api.startRecording());
    } catch (e) {
      showError(e);
    }
  };

  useEffect(() => {
    refresh().catch(showError);
    api.doctor().then(setDoctor);
    api.islandState().then(setIsland);
  }, [refresh, showError]);

  // Recording state, new recordings and "open this call" requests from the island.
  useEffect(() => {
    const unlisteners = [
      onIsland(setIsland),
      onLibraryChanged(() => refresh()),
      onSelectCall((id) => refresh().then(() => setSelected(id))),
      // A model chosen or downloaded in the settings may fix (or break) the setup.
      onSettingsChanged(() => api.doctor().then(setDoctor)),
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

  const visible = useMemo(() => {
    const words = normalize(query).split(/\s+/).filter(Boolean);
    const matches = (call: Call) => {
      const text = normalize(`${call.title} ${call.folder ?? ""}`);
      return words.every((word) => text.includes(word));
    };
    // A search looks in every folder.
    const inView = calls.filter((call) => (words.length > 0 ? matches(call) : inScope(call, scope)));
    return sortCalls(inView, sort);
  }, [calls, query, scope, sort]);

  const callMenu = (call: Call) =>
    popupMenu([
      {
        text: "Renombrar",
        action: () => {
          setSelected(call.id);
          setRenaming(call.id);
        },
      },
      {
        text: "Mover a",
        items: [
          { text: "Sin carpeta", checked: !call.folder, action: () => moveCall(call, null) },
          ...(folders.length > 0 ? (["separator"] as const) : []),
          ...folders.map(({ name }) => ({
            text: name,
            checked: call.folder === name,
            action: () => moveCall(call, name),
          })),
        ],
      },
      "separator",
      { text: "Mostrar en Finder", action: () => api.getCall(call.id).then((d) => revealItemInDir(d.dir)) },
    ]);

  const sortMenu = () =>
    popupMenu(
      (Object.keys(SORT_LABELS) as Sort[]).map((key) => ({
        text: SORT_LABELS[key],
        checked: key === sort,
        action: () => setSort(key),
      })),
    );

  const current = calls.find((c) => c.id === selected) ?? null;
  const searching = query.trim().length > 0;
  const recording = island.kind === "recording";
  const working = calls.filter((c) => c.status.state === "pending" || c.status.state === "processing").length;

  return (
    <div className={`app ${drag ? "dragging-call" : ""}`}>
      <aside className="sidebar">
        <header className="sidebar-header">
          <h1 className="wordmark">calls</h1>
          <div className="actions">
            <button className="button" onClick={pickFiles}>
              importar
            </button>
            <button
              className={`button ${recording ? "live" : "primary"}`}
              onClick={toggleRecording}
              disabled={!doctor?.calls_audio}
            >
              {recording && <span className="signal-square" />}
              {recording ? "detener" : "grabar"}
            </button>
          </div>
        </header>

        <div className="sidebar-tools">
          <label className="search">
            <SearchIcon />
            <input
              type="search"
              placeholder="Buscar"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setQuery("")}
            />
          </label>
          <button className="icon-button" onClick={sortMenu} title={`Ordenar: ${SORT_LABELS[sort]}`}>
            <SortIcon />
          </button>
          <button className="icon-button" onClick={() => api.openSettings()} title="Ajustes (⌘,)">
            <SlidersIcon />
          </button>
        </div>

        <div className="sidebar-scroll">
          {!searching && (
            <Folders
              folders={folders}
              calls={calls}
              scope={scope}
              onScope={setScope}
              dropTarget={drag?.target}
              onChanged={refresh}
              onError={showError}
            />
          )}
          <CallList
            calls={visible}
            sort={sort}
            selected={selected}
            progress={progress}
            folders={folders}
            showFolder={searching || scope.kind === "all"}
            dragging={drag?.call.id}
            emptyText={
              searching
                ? "Ninguna llamada coincide."
                : calls.length === 0
                  ? "Todavía no hay llamadas."
                  : "No hay llamadas acá. Arrastrá alguna a esta carpeta."
            }
            onSelect={(id) => !justDropped() && setSelected(id)}
            onDragStart={startDrag}
            onContextMenu={callMenu}
          />
        </div>
      </aside>

      <main className="content">
        {doctor && <SetupBanner doctor={doctor} />}
        {error && (
          <div className="banner error banner-dismiss" onClick={() => setError(null)}>
            <div className="banner-rail tech-label">
              <span>error</span>
              <span>cerrar ×</span>
            </div>
            <div className="banner-body">{error}</div>
          </div>
        )}
        {current ? (
          <CallView
            call={current}
            progress={progress[current.id]}
            folders={folders}
            renameRequested={renaming === current.id}
            onRenameStarted={() => setRenaming(null)}
            onChanged={refresh}
            onError={showError}
          />
        ) : (
          <section className="empty">
            <p className="empty-label tech-label">sin llamadas</p>
            <p className="empty-title">Arrastrá una grabación acá.</p>
            <p className="empty-hint">Video o audio: OBS, Zoom, Meet, notas de voz…</p>
            <button className="button" onClick={pickFiles}>
              elegir archivos
            </button>
          </section>
        )}
      </main>

      <footer className="statusbar tech-label">
        <span>calls — transcripción local</span>
        <span className="statusbar-state">
          {recording ? (
            <>
              <span className="signal-square" />
              grabando
            </>
          ) : working > 0 ? (
            <>
              <span className="spinner" />
              {working} en proceso
            </>
          ) : (
            plural(calls.length, "llamada", "llamadas")
          )}
        </span>
      </footer>

      {dragging && (
        <div className="drop-overlay">
          <p className="tech-label">soltá para transcribir</p>
        </div>
      )}

      {drag && (
        <div className="drag-ghost" style={{ left: drag.x + 12, top: drag.y + 8 }}>
          {drag.call.title}
        </div>
      )}
    </div>
  );
}
