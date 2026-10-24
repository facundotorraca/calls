import type { Call, CallId } from "./api";
import { formatDate, formatDuration, stepLabel } from "./format";

export type ProgressMap = Record<CallId, { step: string; fraction: number }>;

interface Props {
  calls: Call[];
  selected: CallId | null;
  progress: ProgressMap;
  onSelect: (id: CallId) => void;
}

export function CallList({ calls, selected, progress, onSelect }: Props) {
  if (calls.length === 0) {
    return <p className="muted list-empty">Todavía no hay llamadas.</p>;
  }

  return (
    <ul className="call-list">
      {calls.map((call) => (
        <li key={call.id}>
          <button className={`call-item ${call.id === selected ? "selected" : ""}`} onClick={() => onSelect(call.id)}>
            <span className="call-title">{call.title}</span>
            <span className="call-meta">
              {formatDate(call.created_at)}
              {call.duration_ms != null && ` · ${formatDuration(call.duration_ms)}`}
            </span>
            <CallStatus call={call} progress={progress[call.id]} />
          </button>
        </li>
      ))}
    </ul>
  );
}

function CallStatus({ call, progress }: { call: Call; progress?: ProgressMap[CallId] }) {
  switch (call.status.state) {
    case "ready":
      return null;
    case "failed":
      return <span className="badge error">Error</span>;
    case "recording":
      return <span className="badge live">● Grabando</span>;
    case "pending":
      return <span className="badge">En cola</span>;
    case "processing": {
      const fraction = progress?.fraction ?? 0;
      return (
        <span className="mini-progress">
          <span className="mini-progress-label">
            {stepLabel(call.status.step)} {Math.round(fraction * 100)}%
          </span>
          <span className="bar">
            <span style={{ width: `${fraction * 100}%` }} />
          </span>
        </span>
      );
    }
  }
}
