import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, type CallId, type Mark, type Notes } from "./api";
import { formatTimestamp } from "./format";

interface Props {
  callId: CallId;
  notes: Notes;
  /** The call is being recorded: a moment is marked now, live. */
  recording: boolean;
  /** Where the player is, for marking a moment while listening. */
  position: () => number;
  onSeek?: (ms: number) => void;
  onMarks: (notes: Notes) => void;
  onError: (error: unknown) => void;
}

/** How long typing has to stop before the text is saved. */
const SAVE_AFTER_MS = 600;

/** The user's own notes on a call: free text, saved as it is typed, and marked moments. */
export function NotesView({ callId, notes, recording, position, onSeek, onMarks, onError }: Props) {
  const [saved, setSaved] = useState<"saved" | "saving" | null>(null);
  const [focusMark, setFocusMark] = useState<number | null>(null);

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
    <div className="notes">
      <NoteText key={callId} callId={callId} initial={notes.text} onState={setSaved} onError={onError} />
      <p className="notes-foot tech-label">
        <span>las notas guían el resumen</span>
        <span>{saved === "saving" ? "guardando…" : saved === "saved" ? "guardado" : ""}</span>
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

/** The free text. Saved a moment after typing stops, and on leaving the call. */
function NoteText({
  callId,
  initial,
  onState,
  onError,
}: {
  callId: CallId;
  initial: string;
  onState: (state: "saved" | "saving" | null) => void;
  onError: (error: unknown) => void;
}) {
  const [text, setText] = useState(initial);
  const area = useRef<HTMLTextAreaElement>(null);
  const pending = useRef<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const flush = () => {
    clearTimeout(timer.current);
    const value = pending.current;
    if (value == null) return;
    pending.current = null;
    api.saveNotesText(callId, value).then(
      () => pending.current == null && onState("saved"),
      (e) => {
        onState(null);
        onError(e);
      },
    );
  };

  // Leaving the call (or the tab) saves what is left. Mounted once per call.
  const flushRef = useRef(flush);
  flushRef.current = flush;
  useEffect(() => () => flushRef.current(), []);

  // As tall as its text: the page scrolls, not a box inside it.
  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [text]);

  return (
    <textarea
      ref={area}
      className="notes-text"
      value={text}
      placeholder="Lo que no querés olvidar de esta llamada…"
      spellCheck
      onChange={(e) => {
        setText(e.target.value);
        pending.current = e.target.value;
        onState("saving");
        clearTimeout(timer.current);
        timer.current = setTimeout(flush, SAVE_AFTER_MS);
      }}
      onBlur={flush}
    />
  );
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
