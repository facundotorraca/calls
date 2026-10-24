import { useEffect, useRef, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type Call, type CallDetail, type Folder } from "./api";
import type { ProgressMap } from "./CallList";
import { formatDate, formatDuration, stepLabel } from "./format";
import { folderColor, FolderMark } from "./Folders";
import { ChevronIcon, FinderIcon, PencilIcon } from "./icons";
import { CopyButton, TabPanel, TabRail, type Tab } from "./Tabs";
import { plural, summaryMeta, summaryText, SummaryView } from "./SummaryView";
import { speakers, transcriptText, TranscriptView } from "./TranscriptView";
import { popupMenu, usePersistentState } from "./ui";

interface Props {
  call: Call;
  progress?: ProgressMap[string];
  folders: Folder[];
  /** Switch the title to editing ("Renombrar" in the sidebar), then call `onRenameStarted`. */
  renameRequested: boolean;
  onRenameStarted: () => void;
  onChanged: () => Promise<void>;
  onError: (error: unknown) => void;
}

export function CallView({ call, progress, folders, renameRequested, onRenameStarted, onChanged, onError }: Props) {
  const [detail, setDetail] = useState<CallDetail | null>(null);

  // Reload when switching calls or when processing moves it to a new state.
  useEffect(() => {
    let cancelled = false;
    api.getCall(call.id).then((d) => !cancelled && setDetail(d));
    return () => {
      cancelled = true;
    };
  }, [call.id, call.status.state]);

  const current = detail?.call.id === call.id ? detail : null;
  const transcript = current?.transcript ?? null;
  const summary = current?.summary ?? null;
  const state = call.status.state;
  const summarizing = state === "processing" && call.status.step === "summarize";
  // The open tab is remembered across calls: reading summaries one after another stays on summaries.
  const [tabId, setTabId] = usePersistentState("call.tab", "summary");

  const tabs: Tab[] = [];
  if (transcript && (summary || summarizing || state === "ready")) {
    tabs.push({
      id: "summary",
      label: "resumen",
      meta: summary && summaryMeta(summary),
      actions: summary && <CopyButton text={() => summaryText(summary)} />,
      content: summary ? (
        <SummaryView summary={summary} />
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
  if (transcript) {
    const people = speakers(transcript).length;
    const last = transcript.segments[transcript.segments.length - 1];
    tabs.push({
      id: "transcript",
      label: "transcripción",
      meta: [last && formatDuration(last.end_ms), people > 1 && plural(people, "hablante", "hablantes")]
        .filter(Boolean)
        .join(" · "),
      actions: <CopyButton text={() => transcriptText(transcript)} />,
      content: <TranscriptView transcript={transcript} />,
    });
  }
  const activeTab = tabs.find((t) => t.id === tabId) ?? tabs[0];

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
        <div className="call-rail tech-label">
          <button className="folder-chip tech-label" onClick={folderMenu} title="Mover a otra carpeta">
            <FolderMark color={folderColor(folders, call.folder)} />
            {call.folder ?? "sin carpeta"}
            <ChevronIcon className="folder-chip-chevron" />
          </button>
          <span className="call-rail-meta">
            {formatDate(call.created_at)}
            {call.duration_ms != null && ` · ${formatDuration(call.duration_ms)}`}
            {transcript?.language && ` · ${transcript.language}`}
          </span>
        </div>
        <div className="call-title-row">
          <EditableTitle
            key={call.id}
            title={call.title}
            editRequested={renameRequested}
            onEditStarted={onRenameStarted}
            onSave={rename}
          />
          {current && (
            <button className="ghost-button tech-label" onClick={() => revealItemInDir(current.dir)}>
              <FinderIcon />
              finder
            </button>
          )}
        </div>
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
    </article>
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
