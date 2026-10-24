import type { PointerEvent } from "react";
import type { Call, CallId } from "./api";
import { FolderIcon } from "./icons";
import { formatDate, formatDuration, stepLabel } from "./format";

export type ProgressMap = Record<CallId, { step: string; fraction: number }>;

export type Sort = "newest" | "oldest" | "title" | "longest";

export const SORT_LABELS: Record<Sort, string> = {
  newest: "Más recientes",
  oldest: "Más antiguas",
  title: "Título (A–Z)",
  longest: "Más largas",
};

export function sortCalls(calls: Call[], sort: Sort): Call[] {
  const sorted = [...calls];
  switch (sort) {
    case "newest":
      return sorted.sort((a, b) => b.created_at.localeCompare(a.created_at));
    case "oldest":
      return sorted.sort((a, b) => a.created_at.localeCompare(b.created_at));
    case "title":
      return sorted.sort((a, b) => a.title.localeCompare(b.title, "es", { sensitivity: "base", numeric: true }));
    case "longest":
      return sorted.sort((a, b) => (b.duration_ms ?? 0) - (a.duration_ms ?? 0));
  }
}

interface Props {
  calls: Call[];
  sort: Sort;
  selected: CallId | null;
  progress: ProgressMap;
  /** Show each call's folder (when listing several folders at once). */
  showFolder: boolean;
  /** The call being dragged, dimmed in the list. */
  dragging?: CallId;
  emptyText: string;
  onSelect: (id: CallId) => void;
  onDragStart: (call: Call, event: PointerEvent) => void;
  onContextMenu: (call: Call) => void;
}

export function CallList(props: Props) {
  const { calls, sort, emptyText } = props;
  if (calls.length === 0) {
    return <p className="muted list-empty">{emptyText}</p>;
  }

  // Sorted by date, the list reads better split by when the calls happened.
  const groups = sort === "newest" || sort === "oldest" ? groupByDate(calls) : [{ label: null, calls }];

  return (
    <div className="call-groups">
      {groups.map((group, i) => (
        <section key={group.label ?? i}>
          {group.label && <h3 className="group-label">{group.label}</h3>}
          <ul className="call-list">
            {group.calls.map((call) => (
              <CallItem key={call.id} call={call} {...props} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

function CallItem({
  call,
  selected,
  progress,
  showFolder,
  dragging,
  onSelect,
  onDragStart,
  onContextMenu,
}: Props & { call: Call }) {
  const classes = ["call-item", call.id === selected && "selected", call.id === dragging && "dragging"];
  return (
    <li>
      <button
        className={classes.filter(Boolean).join(" ")}
        onClick={() => onSelect(call.id)}
        onPointerDown={(e) => onDragStart(call, e)}
        onContextMenu={(e) => {
          e.preventDefault();
          onContextMenu(call);
        }}
      >
        <span className="call-title">{call.title}</span>
        <span className="call-meta">
          {formatDate(call.created_at)}
          {call.duration_ms != null && ` · ${formatDuration(call.duration_ms)}`}
        </span>
        {showFolder && call.folder && (
          <span className="call-folder">
            <FolderIcon />
            {call.folder}
          </span>
        )}
        <CallStatus call={call} progress={progress[call.id]} />
      </button>
    </li>
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

const monthFormat = new Intl.DateTimeFormat("es-AR", { month: "long", year: "numeric" });

/** Consecutive calls under "Hoy", "Ayer", "Últimos 7 días" or their month. */
function groupByDate(calls: Call[]): { label: string; calls: Call[] }[] {
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const day = 24 * 60 * 60 * 1000;

  const label = (iso: string) => {
    const date = new Date(iso);
    const start = new Date(date);
    start.setHours(0, 0, 0, 0);
    const daysAgo = Math.round((today.getTime() - start.getTime()) / day);
    if (daysAgo <= 0) return "Hoy";
    if (daysAgo === 1) return "Ayer";
    if (daysAgo < 7) return "Últimos 7 días";
    const month = monthFormat.format(date);
    return month.charAt(0).toUpperCase() + month.slice(1);
  };

  const groups: { label: string; calls: Call[] }[] = [];
  for (const call of calls) {
    const name = label(call.created_at);
    const last = groups[groups.length - 1];
    if (last?.label === name) last.calls.push(call);
    else groups.push({ label: name, calls: [call] });
  }
  return groups;
}
