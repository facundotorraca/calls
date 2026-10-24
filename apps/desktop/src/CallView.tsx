import { useEffect, useRef, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type Call, type CallDetail } from "./api";
import type { ProgressMap } from "./CallList";
import { formatDate, formatDuration, stepLabel } from "./format";
import { ChevronIcon, FinderIcon, FolderIcon, PencilIcon, SparklesIcon, TextIcon } from "./icons";
import { CopyButton, Section } from "./Section";
import { plural, summaryMeta, summaryText, SummaryView } from "./SummaryView";
import { speakers, transcriptText, TranscriptView } from "./TranscriptView";
import { popupMenu } from "./ui";

interface Props {
  call: Call;
  progress?: ProgressMap[string];
  folders: string[];
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
      ...folders.map((name) => ({ text: name, checked: call.folder === name, action: () => move(name) })),
    ]);

  return (
    <article className="call-view">
      <header className="call-header">
        <button className="folder-chip" onClick={folderMenu} title="Mover a otra carpeta">
          <FolderIcon />
          {call.folder ?? "Sin carpeta"}
          <ChevronIcon className="folder-chip-chevron" />
        </button>
        <EditableTitle
          key={call.id}
          title={call.title}
          editRequested={renameRequested}
          onEditStarted={onRenameStarted}
          onSave={rename}
        />
        <div className="call-subheader">
          <p className="muted">
            {formatDate(call.created_at)}
            {call.duration_ms != null && ` · ${formatDuration(call.duration_ms)}`}
            {transcript?.language && ` · ${transcript.language.toUpperCase()}`}
          </p>
          {current && (
            <button className="ghost-button" onClick={() => revealItemInDir(current.dir)}>
              <FinderIcon />
              Mostrar en Finder
            </button>
          )}
        </div>
      </header>

      {call.status.state === "failed" && (
        <div className="banner error">
          <strong>Falló: {stepLabel(call.status.step)}</strong>
          <pre>{call.status.error}</pre>
          <button className="button" onClick={() => api.processCall(call.id)}>
            Reintentar
          </button>
        </div>
      )}

      {state === "recording" && (
        <div className="processing">
          <p>Grabando… la transcripción empieza al terminar la llamada.</p>
        </div>
      )}

      {(state === "pending" || state === "processing") && !summarizing && (
        <div className="processing">
          <p>{call.status.state === "processing" ? stepLabel(call.status.step) : "En cola"}…</p>
          <div className="bar large">
            <span style={{ width: `${(progress?.fraction ?? 0) * 100}%` }} />
          </div>
        </div>
      )}

      {transcript && (summary || summarizing || state === "ready") && (
        <Section
          id="summary"
          icon={<SparklesIcon />}
          title="Resumen"
          meta={summary && summaryMeta(summary)}
          actions={summary && <CopyButton text={() => summaryText(summary)} />}
        >
          {summary ? (
            <SummaryView summary={summary} />
          ) : summarizing ? (
            <div className="section-placeholder">
              <span className="spinner" />
              Resumiendo la llamada…
            </div>
          ) : (
            <div className="section-placeholder">
              Esta llamada todavía no tiene resumen.
              <button className="button" onClick={() => api.processCall(call.id)}>
                Generar resumen
              </button>
            </div>
          )}
        </Section>
      )}

      {transcript && (
        <Section
          id="transcript"
          icon={<TextIcon />}
          title="Transcripción"
          meta={[
            transcript.segments.length > 0 && formatDuration(transcript.segments[transcript.segments.length - 1].end_ms),
            speakers(transcript).length > 1 && plural(speakers(transcript).length, "hablante", "hablantes"),
          ]
            .filter(Boolean)
            .join(" · ")}
          actions={<CopyButton text={() => transcriptText(transcript)} />}
        >
          <TranscriptView transcript={transcript} />
        </Section>
      )}
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
