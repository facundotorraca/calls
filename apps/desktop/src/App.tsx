import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  onDownload,
  me,
  MEDIA_EXTENSIONS,
  type Call,
  type CallId,
  type Doctor,
  type Folder,
  type Island,
  type Settings,
} from "./api";

const NO_NOTION: Settings["notion"] = { workspace: null, parent: null };
import { CallList, SORT_LABELS, sortCalls, type ProgressMap, type Sort } from "./CallList";
import { CallView } from "./CallView";
import { Dashboard } from "./Dashboard";
import { useCallDrag } from "./drag";
import { Folders, inScope, scopeLabel, type Scope } from "./Folders";
import { SearchIcon, SlidersIcon, SortIcon } from "./icons";
import { queryWords, SearchResults, useSearch, type Focus } from "./Search";
import { SetupBanner } from "./SetupBanner";
import { plural } from "./SummaryView";
import { normalize, popupMenu, usePersistentState } from "./ui";

export function App() {
  const [calls, setCalls] = useState<Call[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [selected, setSelected] = useState<CallId | null>(null);
  const [progress, setProgress] = useState<ProgressMap>({});
  const [doctor, setDoctor] = useState<Doctor | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
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
    // A call that went (trashed, or removed in the Finder) leaves its dashboard behind.
    setSelected((current) => (current && list.some((c) => c.id === current) ? current : null));
  }, []);

  // Picking a folder shows its dashboard; a call opens only when it is clicked.
  const changeScope = useCallback(
    (next: Scope) => {
      setScope(next);
      setSelected(null);
    },
    [setScope],
  );

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
      else await api.startRecording();
    } catch (e) {
      showError(e);
    }
  };

  useEffect(() => {
    refresh().catch(showError);
    api.doctor().then(setDoctor);
    api.getSettings().then(setSettings);
    api.islandState().then(setIsland);
  }, [refresh, showError]);

  // Recording state, new recordings and "open this call" requests from the island.
  useEffect(() => {
    const unlisteners = [
      onIsland(setIsland),
      onLibraryChanged(() => refresh()),
      onSelectCall((id) => refresh().then(() => setSelected(id))),
      // A model chosen or downloaded in the settings may fix (or break) the setup.
      onSettingsChanged(() => {
        api.doctor().then(setDoctor);
        api.getSettings().then(setSettings);
      }),
      onDownload((e) => e.state === "done" && api.doctor().then(setDoctor)),
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

  // A search looks in every folder and inside every call (Rust side). Until its first answer,
  // titles that match fill the list so typing never shows it empty.
  const search = useSearch(query);
  const visible = useMemo(() => {
    const words = normalize(query).split(/\s+/).filter(Boolean);
    if (words.length === 0) return sortCalls(calls.filter((call) => inScope(call, scope)), sort);
    if (search.results) {
      const found = new Set(search.results.map((r) => r.id));
      return sortCalls(calls.filter((call) => found.has(call.id)), sort);
    }
    const matches = (call: Call) => {
      const text = normalize(`${call.title} ${call.folder ?? ""}`);
      return words.every((word) => text.includes(word));
    };
    return sortCalls(calls.filter(matches), sort);
  }, [calls, query, scope, sort, search.results]);

  // A call opened from a search result shows where the words are.
  const [focus, setFocus] = useState<Focus | null>(null);
  const openCall = useCallback((id: CallId, at?: Focus) => {
    setSelected(id);
    setFocus(at ?? null);
  }, []);
  const searchInput = useRef<HTMLInputElement>(null);

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
      "separator",
      {
        text: "Trasladar a la papelera",
        accelerator: "CmdOrCtrl+Backspace",
        enabled: !busy(call),
        action: () => trashCall(call),
      },
    ]);

  // What is being written into can not go.
  const busy = (call: Call) => call.status.state === "recording" || call.status.state === "processing";

  // Like the Finder: no confirmation, it can be taken back out of the Trash.
  const trashCall = async (call: Call) => {
    try {
      await api.trashCall(call.id);
      if (selected === call.id) setSelected(null);
      await refresh();
    } catch (e) {
      showError(e);
    }
  };


  const sortMenu = () =>
    popupMenu(
      (Object.keys(SORT_LABELS) as Sort[]).map((key) => ({
        text: SORT_LABELS[key],
        checked: key === sort,
        action: () => setSort(key),
      })),
    );

  const current = calls.find((c) => c.id === selected) ?? null;

  // ⌘F searches; ⌘⌫ trashes the open call and Escape goes back to the dashboard, outside of text fields.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === "f") {
        e.preventDefault();
        searchInput.current?.select();
        return;
      }
      if (!current || (e.target as HTMLElement).closest("input, textarea, [contenteditable]")) return;
      if (e.key === "Escape" && !e.metaKey) {
        setSelected(null);
        return;
      }
      if (!e.metaKey || e.key !== "Backspace" || busy(current)) return;
      e.preventDefault();
      void trashCall(current);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
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
              ref={searchInput}
              type="search"
              placeholder="Buscar en todo  ⌘F"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                // Typing shows the results; a call opens from them.
                setSelected(null);
              }}
              onKeyDown={(e) => e.key === "Escape" && setQuery("")}
            />
            <button className="search-action" onClick={sortMenu} title={`Ordenar: ${SORT_LABELS[sort]}`}>
              <SortIcon />
            </button>
          </label>
        </div>

        <div className="sidebar-scroll">
          {!searching && (
            <Folders
              folders={folders}
              calls={calls}
              scope={scope}
              onScope={changeScope}
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
            onSelect={(id) => !justDropped() && openCall(id)}
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
            canDiarize={doctor?.diarization ?? false}
            me={settings ? me(settings) : "Yo"}
            notion={settings?.notion ?? NO_NOTION}
            progress={progress[current.id]}
            folders={folders}
            back={searching ? "búsqueda" : scopeLabel(scope)}
            onBack={() => setSelected(null)}
            focus={focus}
            highlight={searching ? queryWords(query) : undefined}
            renameRequested={renaming === current.id}
            onRenameStarted={() => setRenaming(null)}
            onChanged={refresh}
            onError={showError}
          />
        ) : searching ? (
          <SearchResults
            query={query}
            results={search.results}
            pending={search.pending}
            calls={calls}
            folders={folders}
            onOpen={openCall}
          />
        ) : (
          loaded && (
            <Dashboard
              key={JSON.stringify(scope)}
              scope={scope}
              calls={calls}
              folders={folders}
              progress={progress}
              onOpen={openCall}
              onScope={changeScope}
              onImport={pickFiles}
            />
          )
        )}
      </main>

      <footer className="statusbar tech-label">
        <button className="statusbar-button" onClick={() => api.openSettings()} title="Ajustes">
          <SlidersIcon />
          ajustes <kbd>⌘,</kbd>
        </button>
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
