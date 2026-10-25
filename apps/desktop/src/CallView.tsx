import { useCallback, useEffect, useRef, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, onNotesChanged, type Call, type CallDetail, type Folder, type Notes, type Segment, type Settings } from "./api";
import type { ProgressMap } from "./CallList";
import { formatDuration, stepLabel } from "./format";
import { NotesView } from "./NotesView";
import { NotionControl } from "./Notion";
import type { Focus } from "./Search";
import { folderColor, FolderMark } from "./Folders";
import { ChevronIcon, FinderIcon, PencilIcon } from "./icons";
import { Player, type PlayerHandle } from "./Player";
import { CopyButton, TabPanel, TabRail, type Tab } from "./Tabs";
import { plural, summaryMeta, summaryText, SummaryView } from "./SummaryView";
import { segmentAt, speakers, transcriptText, TranscriptView } from "./TranscriptView";
import { popupMenu, usePersistentState } from "./ui";

interface Props {
  call: Call;
  /** Diarization is on and its model downloaded: offer it for calls processed without it. */
  canDiarize: boolean;
  /** The speaker label of the person using the app. */
  me: string;
  notion: Settings["notion"];
  progress?: ProgressMap[string];
  folders: Folder[];
  /** The name of the dashboard the call was opened from, for the way back. */
  back: string;
  onBack: () => void;
  /** Opened from a search: the tab and moment to show. */
  focus?: Focus | null;
  /** The words searched for, marked in the summary and the transcript. */
  highlight?: string[];
  /** Switch the title to editing ("Renombrar" in the sidebar), then call `onRenameStarted`. */
  renameRequested: boolean;
  onRenameStarted: () => void;
  onChanged: () => Promise<void>;
  onError: (error: unknown) => void;
}

export function CallView({
  call,
  canDiarize,
  me,
  notion,
  progress,
  folders,
  back,
  onBack,
  focus,
  highlight,
  renameRequested,
  onRenameStarted,
  onChanged,
  onError,
}: Props) {
  const [detail, setDetail] = useState<CallDetail | null>(null);
  const [edits, setEdits] = useState(0);

  // Reload when switching calls, when processing moves it to a new state, or after an edit.
  useEffect(() => {
    let cancelled = false;
    api.getCall(call.id).then((d) => !cancelled && setDetail(d));
    return () => {
      cancelled = true;
    };
  }, [call.id, call.status.state, edits]);

  // The user's notes, reloaded when a moment is marked from the island.
  const [notes, setNotes] = useState<{ id: string; notes: Notes } | null>(null);
  useEffect(() => {
    let cancelled = false;
    const load = () => api.getNotes(call.id).then((n) => !cancelled && setNotes({ id: call.id, notes: n }), onError);
    load();
    const unlisten = onNotesChanged((id) => id === call.id && load());
    return () => {
      cancelled = true;
      void unlisten.then((fn) => fn());
    };
  }, [call.id, onError]);
  const callNotes = notes?.id === call.id ? notes.notes : null;

  const current = detail?.call.id === call.id ? detail : null;
  const transcript = current?.transcript ?? null;
  const summary = current?.summary ?? null;
  const state = call.status.state;
  const summarizing = state === "processing" && call.status.step === "summarize";
  // The open tab is remembered across calls: reading summaries one after another stays on summaries.
  const [tabId, setTabId] = usePersistentState("call.tab", "summary");

  // The player reports its position every frame; only a change of segment re-renders.
  const player = useRef<PlayerHandle>(null);
  const [playing, setPlaying] = useState<Segment | null>(null);
  const time = useRef(0);
  const onTime = useCallback(
    (ms: number) => {
      time.current = ms;
      setPlaying(transcript ? segmentAt(transcript.segments, ms) : null);
    },
    [transcript],
  );
  const position = useCallback(() => time.current, []);
  useEffect(() => {
    time.current = 0;
  }, [call.id]);
  const seek = useCallback((ms: number) => player.current?.seek(ms), []);

  const renameSpeaker = useCallback(
    (from: string, to: string) =>
      api.renameSpeaker(call.id, from, to).then(
        () => setEdits((n) => n + 1),
        (e) => onError(e),
      ),
    [call.id, onError],
  );
  const diarizable = canDiarize && state === "ready" && !("diarize" in call.completed_steps);

  const tabs: Tab[] = [];
  if (transcript && (summary || summarizing || state === "ready")) {
    tabs.push({
      id: "summary",
      label: "resumen",
      meta: summary && summaryMeta(summary),
      actions: summary && <CopyButton text={() => summaryText(summary)} />,
      content: summary ? (
        <SummaryView summary={summary} highlight={highlight} />
      ) : summarizing ? (
        <div className="tab-placeholder">
          <span className="spinner" />
          Resumiendo la llamada…
        </div>
      ) : (
        <div className="tab-placeholder">
          Esta llamada todavía no tiene resumen.
          <button className="button" onClick={() => api.processCall(call.id)}>
            generar resumen
          </button>
        </div>
      ),
    });
  }
  if (callNotes) {
    const marks = callNotes.marks.length;
    tabs.push({
      id: "notes",
      label: "notas",
      meta: marks > 0 && plural(marks, "momento", "momentos"),
      content: (
        <NotesView
          callId={call.id}
          notes={callNotes}
          recording={state === "recording"}
          position={position}
          onSeek={seek}
          onMarks={(n) => setNotes({ id: call.id, notes: n })}
          onError={onError}
        />
      ),
    });
  }
  if (transcript) {
    const people = speakers(transcript).length;
    const last = transcript.segments[transcript.segments.length - 1];
    tabs.push({
      id: "transcript",
      label: "transcripción",
      meta: [last && formatDuration(last.end_ms), people > 1 && plural(people, "hablante", "hablantes")]
        .filter(Boolean)
        .join(" · "),
      actions: (
        <>
          {diarizable && (
            <button className="ghost-button tech-label" onClick={() => api.processCall(call.id).then(onChanged)}>
              separar hablantes
            </button>
          )}
          <CopyButton text={() => transcriptText(transcript)} />
        </>
      ),
      content: (
        <TranscriptView
          transcript={transcript}
          active={playing}
          marks={callNotes?.marks}
          highlight={highlight}
          onSeek={seek}
          onRenameSpeaker={renameSpeaker}
          me={me}
        />
      ),
    });
  }
  const activeTab = tabs.find((t) => t.id === tabId) ?? tabs[0];

  // From a search: open its tab, then, once the passage is on the page, bring it into view.
  const pendingFocus = useRef<Focus | null>(null);
  useEffect(() => {
    if (!focus) return;
    pendingFocus.current = focus;
    setTabId(focus.tab);
  }, [focus, setTabId]);
  useEffect(() => {
    const target = pendingFocus.current;
    if (!target || activeTab?.id !== target.tab) return;
    if (target.at_ms == null) {
      pendingFocus.current = null;
      return;
    }
    const el = document.querySelector(`[data-start="${target.at_ms}"], [data-mark="${target.at_ms}"]`);
    if (!el) return;
    pendingFocus.current = null;
    el.scrollIntoView({ block: "center" });
    el.classList.remove("flash");
    void (el as HTMLElement).offsetWidth;
    el.classList.add("flash");
  });

  const rename = async (title: string) => {
    try {
      await api.renameCall(call.id, title);
      await onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const move = async (folder: string | null) => {
    try {
      await api.moveCall(call.id, folder);
      await onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const folderMenu = () =>
    popupMenu([
      { text: "Sin carpeta", checked: !call.folder, action: () => move(null) },
      ...(folders.length > 0 ? (["separator"] as const) : []),
      ...folders.map(({ name }) => ({ text: name, checked: call.folder === name, action: () => move(name) })),
    ]);

  return (
    <article className="call-view">
      <header className="call-header dot-grid">
        <div className="call-rail">
          <button className="ghost-button tech-label back-button" onClick={onBack} title="Volver (esc)">
            <ChevronIcon className="chevron-back" />
            {back}
          </button>
          <span className="call-rail-sep" aria-hidden />
          <button
            className="folder-chip tech-label"
            data-color={folderColor(folders, call.folder)}
            onClick={folderMenu}
            title="Mover a otra carpeta"
          >
            <FolderMark color={folderColor(folders, call.folder)} />
            {call.folder ?? "sin carpeta"}
            <ChevronIcon className="folder-chip-chevron" />
          </button>
          {current && (
            <button className="ghost-button tech-label" onClick={() => revealItemInDir(current.dir)}>
              <FinderIcon />
              finder
            </button>
          )}
          <span className="call-rail-spacer" />
          <NotionControl call={call} notion={notion} onError={onError} />
        </div>
        <div className="call-title-row">
          <EditableTitle
            key={call.id}
            title={call.title}
            editRequested={renameRequested}
            onEditStarted={onRenameStarted}
            onSave={rename}
          />
        </div>
        <dl className="readouts">
          <Readout label="fecha" value={dayFormat.format(new Date(call.created_at)).replace(",", "")} />
          <Readout label="hora" value={timeFormat.format(new Date(call.created_at))} />
          {call.duration_ms != null && <Readout label="duración" value={formatDuration(call.duration_ms)} />}
          {transcript?.language && <Readout label="idioma" value={languageName(transcript.language)} />}
          {transcript && speakers(transcript).length > 1 && (
            <Readout label="hablantes" value={String(speakers(transcript).length)} />
          )}
        </dl>
      </header>

      {activeTab && <TabRail tabs={tabs} active={activeTab} onSelect={setTabId} />}

      <div className="call-body">
        {call.status.state === "failed" && (
          <div className="banner error">
            <div className="banner-rail tech-label">
              <span>falló: {stepLabel(call.status.step)}</span>
            </div>
            <div className="banner-body">
              <pre>{call.status.error}</pre>
              <button className="button" onClick={() => api.processCall(call.id)}>
                reintentar
              </button>
            </div>
          </div>
        )}

        {state === "recording" && (
          <div className="processing">
            <p className="processing-label tech-label">
              <span className="signal-square" />
              grabando
            </p>
            <p>La transcripción empieza al terminar la llamada.</p>
          </div>
        )}

        {(state === "pending" || state === "processing") && !summarizing && (
          <div className="processing">
            <p className="processing-label tech-label">
              <span>{call.status.state === "processing" ? stepLabel(call.status.step) : "en cola"}</span>
              {progress && progress.fraction > 0 && <span>{Math.round(progress.fraction * 100)}%</span>}
            </p>
            <div className="bar large">
              <span style={{ width: `${(progress?.fraction ?? 0) * 100}%` }} />
            </div>
          </div>
        )}

        {activeTab && <TabPanel tab={activeTab} />}
      </div>

      {state !== "recording" && <Player key={call.id} ref={player} callId={call.id} onTime={onTime} />}
    </article>
  );
}

const dayFormat = new Intl.DateTimeFormat("es-AR", {
  weekday: "short",
  day: "numeric",
  month: "short",
  year: "numeric",
});
const timeFormat = new Intl.DateTimeFormat("es-AR", { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
const languages = new Intl.DisplayNames(["es"], { type: "language" });
const languageName = (code: string) => (languages.of(code) ?? code).toLowerCase();

/** One fact about the call: a quiet label over its value. */
function Readout({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="readout">
      <dt className="tech-label">{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function EditableTitle({
  title,
  editRequested,
  onEditStarted,
  onSave,
}: {
  title: string;
  editRequested: boolean;
  onEditStarted: () => void;
  onSave: (title: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(title);
  const done = useRef(false);

  const startEditing = () => {
    setValue(title);
    done.current = false;
    setEditing(true);
  };

  useEffect(() => {
    if (!editRequested) return;
    startEditing();
    onEditStarted();
  });

  if (!editing) {
    return (
      <h2 className="call-title-text" onClick={startEditing} title="Renombrar">
        {title}
        <PencilIcon className="edit-hint" />
      </h2>
    );
  }

  // Enter, Escape and the blur that follows them must act once.
  const finish = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    setEditing(false);
    const next = value.trim();
    if (save && next && next !== title) onSave(next);
  };

  return (
    <input
      className="title-input"
      value={value}
      autoFocus
      onFocus={(e) => e.target.select()}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => finish(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(true);
        if (e.key === "Escape") finish(false);
      }}
    />
  );
}
