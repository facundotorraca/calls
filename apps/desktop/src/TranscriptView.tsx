import { memo, useEffect, useRef, useState } from "react";
import type { Segment, Transcript } from "./api";
import { formatTimestamp } from "./format";

/** Consecutive segments of one speaker, read as a paragraph. */
interface Turn {
  speaker?: string;
  start_ms: number;
  segments: Segment[];
}

// Without speakers there are no turns: start a paragraph at pauses, or every minute.
const PAUSE_MS = 1500;
const MAX_UNLABELED_MS = 60_000;

export function turns(transcript: Transcript): Turn[] {
  const out: Turn[] = [];
  for (const segment of transcript.segments) {
    const turn = out[out.length - 1];
    const previous = turn?.segments[turn.segments.length - 1];
    const continues =
      turn &&
      previous &&
      turn.speaker === segment.speaker &&
      (segment.speaker !== undefined ||
        (segment.start_ms - previous.end_ms < PAUSE_MS && segment.start_ms - turn.start_ms < MAX_UNLABELED_MS));
    if (continues) turn.segments.push(segment);
    else out.push({ speaker: segment.speaker, start_ms: segment.start_ms, segments: [segment] });
  }
  return out;
}

export function speakers(transcript: Transcript): string[] {
  return [...new Set(transcript.segments.flatMap((s) => (s.speaker ? [s.speaker] : [])))];
}

/** The segment playing at `ms`: the last one started, unless it ended a while ago. */
export function segmentAt(segments: Segment[], ms: number): Segment | null {
  let lo = 0;
  let hi = segments.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (segments[mid].start_ms <= ms) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  const segment = segments[found];
  return segment && ms < segment.end_ms + 1000 ? segment : null;
}

interface Props {
  transcript: Transcript;
  /** The segment playing, highlighted. */
  active?: Segment | null;
  /** Clicking a segment or a timestamp plays from there. */
  onSeek?: (ms: number) => void;
  /** Clicking a speaker's name renames them throughout the call. */
  onRenameSpeaker?: (from: string, to: string) => void;
  /** The label of the person using the app, offered as "soy yo" when renaming. */
  me?: string;
}

/** Re-renders only when the playing segment changes, not on every frame of playback. */
export const TranscriptView = memo(function TranscriptView({ transcript, active, onSeek, onRenameSpeaker, me }: Props) {
  const names = speakers(transcript);
  const activeEl = useRef<HTMLSpanElement>(null);
  const previous = useRef<HTMLSpanElement | null>(null);

  // Follow the playback, but only if you were following it: while the previous segment
  // was on screen. Scrolled away to read something else, the page stays put.
  useEffect(() => {
    const el = activeEl.current;
    if (!el) return;
    const before = previous.current;
    previous.current = el;
    if (before && isVisible(before) && !isVisible(el)) el.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [active]);

  // A click that ends a text selection is a selection, not a seek.
  const seek = (ms: number) => {
    if (!window.getSelection()?.isCollapsed) return;
    onSeek?.(ms);
  };

  return (
    <div className="transcript">
      {turns(transcript).map((turn, i) => {
        const color = turn.speaker ? names.indexOf(turn.speaker) % 6 : undefined;
        return (
          <div key={i} className="turn" data-speaker={color}>
            <button className="turn-time" onClick={() => seek(turn.start_ms)} disabled={!onSeek}>
              {formatTimestamp(turn.start_ms)}
            </button>
            <div className="turn-body">
              {turn.speaker && (
                <div className="turn-speaker">
                  <span className="avatar">{initial(turn.speaker)}</span>
                  <SpeakerName name={turn.speaker} me={me} onRename={onRenameSpeaker} />
                </div>
              )}
              <p>
                {turn.segments.map((segment, j) => (
                  <span
                    key={j}
                    ref={segment === active ? activeEl : undefined}
                    className={segment === active ? "segment active" : "segment"}
                    title={formatTimestamp(segment.start_ms)}
                    onClick={() => seek(segment.start_ms)}
                  >
                    {segment.text}{" "}
                  </span>
                ))}
              </p>
            </div>
          </div>
        );
      })}
    </div>
  );
});

/** "Hablante 2" -> "2"; a name -> its first letter. */
function initial(speaker: string): string {
  return speaker.match(/^Hablante (\d+)$/)?.[1] ?? speaker.charAt(0).toUpperCase();
}

/** The speaker's name; clicked, an input that renames them in the whole call. */
function SpeakerName({
  name,
  me,
  onRename,
}: {
  name: string;
  me?: string;
  onRename?: (from: string, to: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(name);
  const done = useRef(false);

  if (!editing || !onRename) {
    return (
      <button
        className="speaker-name"
        disabled={!onRename}
        title="Renombrar en toda la llamada"
        onClick={() => {
          setValue(name);
          done.current = false;
          setEditing(true);
        }}
      >
        {name}
      </button>
    );
  }

  // Enter, Escape and the blur that follows them must act once.
  const finish = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    setEditing(false);
    const next = value.trim();
    if (save && next && next !== name) onRename(name, next);
  };
  return (
    <span className="speaker-edit">
      <input
        className="speaker-input"
        value={value}
        autoFocus
        size={Math.max(8, value.length + 1)}
        onFocus={(e) => e.target.select()}
        onChange={(e) => setValue(e.target.value)}
        onBlur={() => finish(true)}
        onKeyDown={(e) => {
          if (e.key === "Enter") finish(true);
          if (e.key === "Escape") finish(false);
        }}
      />
      {me && name !== me && (
        // Before the input's blur, which would save what is typed instead.
        <button
          className="ghost-button tech-label"
          title="Este hablante sos vos: la app aprende tu voz para reconocerte en otras llamadas"
          onMouseDown={(e) => {
            e.preventDefault();
            done.current = true;
            setEditing(false);
            onRename(name, me);
          }}
        >
          soy yo
        </button>
      )}
    </span>
  );
}

function isVisible(el: HTMLElement): boolean {
  const scroller = el.closest(".content");
  if (!scroller) return false;
  const box = el.getBoundingClientRect();
  const view = scroller.getBoundingClientRect();
  return box.bottom > view.top && box.top < view.bottom;
}

export function transcriptText(transcript: Transcript): string {
  return turns(transcript)
    .map((turn) => {
      const text = turn.segments.map((s) => s.text).join(" ");
      return `[${formatTimestamp(turn.start_ms)}] ${turn.speaker ? `${turn.speaker}: ` : ""}${text}`;
    })
    .join("\n\n");
}
