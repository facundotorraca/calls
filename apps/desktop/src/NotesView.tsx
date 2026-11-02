import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, isImagePath, type CallId, type Mark, type Notes } from "./api";
import { formatTimestamp } from "./format";
import { NoteEditor, type NoteEditorHandle } from "./NoteEditor";

interface Props {
  callId: CallId;
  /** The call's folder, where the images of the notes are; `null` until known. */
  dir: string | null;
  notes: Notes;
  /** The call is being recorded: a moment is marked now, live. */
  recording: boolean;
  /** Where the player is, for marking a moment while listening. */
  position: () => number;
  onSeek?: (ms: number) => void;
  onMarks: (notes: Notes) => void;
  onError: (error: unknown) => void;
}

/** The user's own notes on a call: free text with images in it, saved as it is typed, and
 * marked moments. Images dropped on them go in where the caret was. */
export function NotesView({ callId, dir, notes, recording, position, onSeek, onMarks, onError }: Props) {
  const [saved, setSaved] = useState<"saved" | "saving" | null>(null);
  const [focusMark, setFocusMark] = useState<number | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const editor = useRef<NoteEditorHandle>(null);

  // `App` leaves image files dropped here to the notes instead of importing them.
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type !== "drop" || !payload.paths.every(isImagePath)) return;
      const scale = window.devicePixelRatio || 1;
      const under = document.elementFromPoint(payload.position.x / scale, payload.position.y / scale);
      if (under && root.current?.contains(under)) void editor.current?.attachFiles(payload.paths).catch(() => {});
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

  const mark = async () => {
    try {
      const at = recording ? undefined : Math.round(position());
      const next = await api.addMark(callId, at);
      onMarks(next);
      // The newest moment is the one just marked while recording; otherwise the one at `at`.
      setFocusMark(at ?? Math.max(...next.marks.map((m) => m.at_ms)));
    } catch (e) {
      onError(e);
    }
  };

  const setMark = (at: number, text: string | null) =>
    api.setMark(callId, at, text).then(onMarks, onError);

  return (
    <div className="notes" ref={root} data-notes-call={callId}>
      <NoteEditor
        key={callId}
        ref={editor}
        callId={callId}
        dir={dir}
        initial={notes.text}
        onState={setSaved}
        onError={onError}
        onSeek={recording ? undefined : onSeek}
      />
      <p className="notes-foot tech-label">
        <span>las notas guían el resumen · ⌘V pega imágenes</span>
        <span className="notes-foot-end">
          {saved === "saving" ? "guardando…" : saved === "saved" ? "guardado" : ""}
          <button className="ghost-button" onClick={() => void editor.current?.pick().catch(() => {})}>
            adjuntar imagen
          </button>
        </span>
      </p>

      <section className="marks">
        <h4 className="marks-rail">
          <span className="tech-label">
            momentos {notes.marks.length > 0 && <span className="count">{notes.marks.length}</span>}
          </span>
          <button className={`button ${recording ? "primary" : ""}`} onClick={mark}>
            {recording ? (
              <>
                <span className="mark-square" />
                marcar momento
              </>
            ) : (
              <MarkHereLabel position={position} />
            )}
          </button>
        </h4>
        {notes.marks.length === 0 ? (
          <p className="marks-empty">
            {recording
              ? "Marcá lo que importa mientras hablan: queda con su hora y aparece en la transcripción."
              : "Marcá un momento mientras escuchás para volver a él."}
          </p>
        ) : (
          <ul className="mark-list">
            {notes.marks.map((m) => (
              <MarkRow
                key={m.at_ms}
                mark={m}
                autoFocus={focusMark === m.at_ms}
                onSeek={recording ? undefined : onSeek}
                onSave={(text) => setMark(m.at_ms, text)}
                onRemove={() => setMark(m.at_ms, null)}
              />
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

/** "marcar en 12:34", following the player. */
function MarkHereLabel({ position }: { position: () => number }) {
  const [ms, setMs] = useState(position());
  useEffect(() => {
    const timer = setInterval(() => setMs(position()), 500);
    return () => clearInterval(timer);
  }, [position]);
  return <>marcar en {formatTimestamp(ms)}</>;
}

function MarkRow({
  mark,
  autoFocus,
  onSeek,
  onSave,
  onRemove,
}: {
  mark: Mark;
  autoFocus: boolean;
  onSeek?: (ms: number) => void;
  onSave: (text: string) => void;
  onRemove: () => void;
}) {
  const [value, setValue] = useState(mark.text);
  useEffect(() => setValue(mark.text), [mark.text]);
  const save = () => value !== mark.text && onSave(value.trim());

  return (
    <li className="mark-row row-tick" data-mark={mark.at_ms}>
      <button className="mark-time" onClick={() => onSeek?.(mark.at_ms)} disabled={!onSeek} title="Escuchar desde acá">
        <span className="mark-square" />
        {formatTimestamp(mark.at_ms)}
      </button>
      <input
        className="mark-input"
        value={value}
        placeholder="¿Qué pasó acá?"
        autoFocus={autoFocus}
        onChange={(e) => setValue(e.target.value)}
        onBlur={save}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            setValue(mark.text);
            e.currentTarget.blur();
          }
        }}
      />
      <button className="ghost-button danger mark-remove" onClick={onRemove} title="Quitar el momento">
        ×
      </button>
    </li>
  );
}
