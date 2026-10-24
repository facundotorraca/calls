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

export function TranscriptView({ transcript }: { transcript: Transcript }) {
  const names = speakers(transcript);

  return (
    <div className="transcript">
      {turns(transcript).map((turn, i) => {
        const color = turn.speaker ? names.indexOf(turn.speaker) % 6 : undefined;
        return (
          <div key={i} className="turn" data-speaker={color}>
            <time className="turn-time">{formatTimestamp(turn.start_ms)}</time>
            <div className="turn-body">
              {turn.speaker && (
                <div className="turn-speaker">
                  <span className="avatar">{turn.speaker.charAt(0).toUpperCase()}</span>
                  {turn.speaker}
                </div>
              )}
              <p>
                {turn.segments.map((segment, j) => (
                  <span key={j} title={formatTimestamp(segment.start_ms)}>
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
}

export function transcriptText(transcript: Transcript): string {
  return turns(transcript)
    .map((turn) => {
      const text = turn.segments.map((s) => s.text).join(" ");
      return `[${formatTimestamp(turn.start_ms)}] ${turn.speaker ? `${turn.speaker}: ` : ""}${text}`;
    })
    .join("\n\n");
}
