import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api, type ActionItem, type Call, type CallId, type Folder, type FolderColor, type Summary } from "./api";
import type { ProgressMap } from "./CallList";
import { folderColor, FolderMark, inScope, scopeLabel, type Scope } from "./Folders";
import { formatDuration, stepLabel } from "./format";
import { CalendarIcon, CheckCircleIcon, FolderIcon, PageIcon, PersonIcon } from "./icons";
import { plural } from "./SummaryView";

interface Props {
  scope: Scope;
  /** Every call; the dashboard keeps the ones in `scope`. */
  calls: Call[];
  folders: Folder[];
  progress: ProgressMap;
  onOpen: (id: CallId) => void;
  onScope: (scope: Scope) => void;
  onImport: () => void;
}

/** How many calls the "recientes" tiles show. Even, so the two columns close flush. */
const RECENT = 6;
/** Pendientes shown before "ver todos". */
const TODOS = 6;

/**
 * The front page of a scope (every call, the unfiled ones or a folder): what it adds
 * up to, what is moving, what was said lately and what is still owed.
 */
export function Dashboard({ scope, calls, folders, progress, onOpen, onScope, onImport }: Props) {
  const summaries = useSummaries(calls);
  const inView = useMemo(
    () => calls.filter((c) => inScope(c, scope)).sort((a, b) => b.created_at.localeCompare(a.created_at)),
    [calls, scope],
  );
  const folder = scope.kind === "folder" ? folders.find((f) => f.name === scope.name) : undefined;
  const color = folder?.color;
  const last = inView[0];

  const todos = inView.flatMap((call) => (summaries[call.id]?.action_items ?? []).map((item) => ({ call, item })));
  const decisions = inView.reduce((n, call) => n + (summaries[call.id]?.decisions.length ?? 0), 0);
  const live = inView.filter((c) => ["recording", "pending", "processing"].includes(c.status.state));

  return (
    <article className="dash">
      <header className="dash-header dot-grid">
        <div className="dash-rail tech-label">
          <span className="dash-crumbs">
            calls<span className="dash-crumb-sep">/</span>
            {scope.kind === "folder" ? (
              <>
                carpetas<span className="dash-crumb-sep">/</span>
                <span className="dash-crumb-here">{scope.name}</span>
              </>
            ) : (
              <span className="dash-crumb-here">{scopeLabel(scope)}</span>
            )}
          </span>
          <span>{last ? `última · ${ago(last.created_at)}` : "vacía"}</span>
        </div>
        <div className="dash-title-row">
          {scope.kind === "folder" && (
            <span className="dash-title-mark" data-color={color}>
              <FolderIcon />
            </span>
          )}
          <h2 className="dash-title">{scopeLabel(scope)}</h2>
          {folder?.notion && (
            <button className="chip dash-notion" onClick={() => api.openSettings("notion")} title="Página de Notion">
              <PageIcon />
              {folder.notion.title}
            </button>
          )}
        </div>
      </header>

      {inView.length === 0 ? (
        <section className="empty dash-empty">
          <p className="empty-label tech-label">{scope.kind === "folder" ? "carpeta vacía" : "sin llamadas"}</p>
          <p className="empty-title">
            {scope.kind === "folder" ? "Arrastrá llamadas a esta carpeta." : "Arrastrá una grabación acá."}
          </p>
          <p className="empty-hint">
            {scope.kind === "folder"
              ? "Desde la lista, o importá grabaciones nuevas: quedan acá."
              : "Video o audio: OBS, Zoom, Meet, notas de voz…"}
          </p>
          <button className="button" onClick={onImport}>
            {scope.kind === "folder" ? "importar acá" : "elegir archivos"}
          </button>
        </section>
      ) : (
        <div className="dash-body">
          <Readouts calls={inView} todos={todos.length} decisions={decisions} summaries={summaries} />

          {live.length > 0 && (
            <Section label="en curso" count={live.length}>
              <ul className="live-list">
                {live.map((call, i) => (
                  <li key={call.id} className="lane-in" style={lane(i)}>
                    <LiveRow call={call} progress={progress[call.id]} onOpen={onOpen} />
                  </li>
                ))}
              </ul>
            </Section>
          )}

          <Activity calls={inView} color={color} onOpen={onOpen} />

          <Section
            label="recientes"
            count={inView.length}
            side={inView.length > RECENT && <span className="tech-label">{`${RECENT} de ${inView.length}`}</span>}
          >
            <div className="tiles">
              {inView.slice(0, RECENT).map((call, i) => (
                <CallTile
                  key={call.id}
                  index={i}
                  call={call}
                  summary={summaries[call.id]}
                  progress={progress[call.id]}
                  folders={folders}
                  showFolder={scope.kind === "all"}
                  onOpen={onOpen}
                />
              ))}
            </div>
          </Section>

          {todos.length > 0 && <Todos todos={todos} onOpen={onOpen} />}

          {scope.kind === "all" && folders.length > 0 && (
            <FolderBreakdown calls={inView} folders={folders} onScope={onScope} />
          )}
        </div>
      )}
    </article>
  );
}

/** Every call's summary, fetched again whenever a call is added, goes or changes state. */
function useSummaries(calls: Call[]): Record<CallId, Summary> {
  const [summaries, setSummaries] = useState<Record<CallId, Summary>>({});
  const signature = calls.map((c) => `${c.id}:${c.status.state}`).join();
  useEffect(() => {
    let cancelled = false;
    api.summaries().then(
      (s) => !cancelled && setSummaries(s),
      () => {},
    );
    return () => {
      cancelled = true;
    };
  }, [signature]);
  return summaries;
}

/** Staggers a row's arrival; capped so the last row never waits on the first. */
const lane = (index: number) => ({ "--lane-index": Math.min(index, 12) }) as CSSProperties;

function Section({
  label,
  count,
  side,
  children,
}: {
  label: string;
  count?: number;
  side?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="dash-section">
      <h3 className="dash-section-rail">
        <span className="tech-label">
          {label}
          {count != null && <span className="count">{count}</span>}
        </span>
        {side && <span className="dash-section-side">{side}</span>}
      </h3>
      {children}
    </section>
  );
}

// ── Readouts ────────────────────────────────────────────────────────────

function Readouts({
  calls,
  todos,
  decisions,
  summaries,
}: {
  calls: Call[];
  todos: number;
  decisions: number;
  summaries: Record<CallId, Summary>;
}) {
  const total = calls.reduce((ms, c) => ms + (c.duration_ms ?? 0), 0);
  const timed = calls.filter((c) => c.duration_ms != null).length;
  const week = calls.filter((c) => daysAgo(c.created_at) < 7).length;
  const withTodos = calls.filter((c) => (summaries[c.id]?.action_items.length ?? 0) > 0).length;
  const withDecisions = calls.filter((c) => (summaries[c.id]?.decisions.length ?? 0) > 0).length;

  return (
    <dl className="stats">
      <Stat
        label="llamadas"
        figure={[[String(calls.length)]]}
        detail={week > 0 ? `${week} en los últimos 7 días` : `la última ${ago(calls[0].created_at)}`}
      />
      <Stat
        label="grabado"
        figure={durationParts(total)}
        detail={timed > 0 ? `promedio ${formatDuration(total / timed)}` : "—"}
      />
      <Stat
        label="pendientes"
        figure={[[String(todos)]]}
        detail={todos > 0 ? `de ${plural(withTodos, "llamada", "llamadas")}` : "nada pendiente"}
        tone={todos > 0 ? "pending" : undefined}
      />
      <Stat
        label="decisiones"
        figure={[[String(decisions)]]}
        detail={decisions > 0 ? `en ${plural(withDecisions, "llamada", "llamadas")}` : "ninguna todavía"}
        tone={decisions > 0 ? "settled" : undefined}
      />
    </dl>
  );
}

/** An instrument figure: a mono label, a hairline drawn on arrival, the value in Doto. */
function Stat({
  label,
  figure,
  detail,
  tone,
}: {
  label: string;
  /** Number and unit pairs, e.g. `[["2", "h"], ["15", "min"]]`. */
  figure: [string, string?][];
  detail: string;
  /** Marks the label, never the value. */
  tone?: "pending" | "settled";
}) {
  return (
    <div className="stat">
      <dt className="stat-label tech-label" data-tone={tone}>
        {tone && <span className="stat-mark" />}
        {label}
      </dt>
      <span className="sweep-rule" aria-hidden />
      <dd className="stat-figure">
        {figure.map(([n, unit], i) => (
          <span key={i}>
            {n}
            {unit && <span className="stat-unit">{unit}</span>}
          </span>
        ))}
      </dd>
      <dd className="stat-detail">{detail}</dd>
    </div>
  );
}

function durationParts(ms: number): [string, string][] {
  const minutes = Math.round(ms / 60000);
  if (minutes < 60) return [[String(minutes), "min"]];
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? [[String(hours), "h"]] : [[String(hours), "h"], [String(rest), "min"]];
}

// ── En curso ────────────────────────────────────────────────────────────

function LiveRow({
  call,
  progress,
  onOpen,
}: {
  call: Call;
  progress?: ProgressMap[CallId];
  onOpen: (id: CallId) => void;
}) {
  const fraction = progress?.fraction ?? 0;
  const { status } = call;
  return (
    <button className="live-row row-tick" onClick={() => onOpen(call.id)}>
      <span className="live-row-title">{call.title}</span>
      <span className="live-row-state tech-label" data-state={status.state}>
        {status.state === "recording" && (
          <>
            <span className="signal-square" />
            grabando
          </>
        )}
        {status.state === "pending" && "en cola"}
        {status.state === "processing" && (
          <>
            <span className="spinner" />
            {stepLabel(status.step)}
            {fraction > 0 && <span className="live-row-pct">{Math.round(fraction * 100)}%</span>}
          </>
        )}
      </span>
      <span className="bar">
        <span style={{ width: `${status.state === "processing" ? fraction * 100 : 0}%` }} />
      </span>
    </button>
  );
}

// ── Actividad: a dot-matrix of days, one column per week ───────────────

interface Day {
  date: Date;
  calls: Call[];
  ms: number;
  today: boolean;
  future: boolean;
}

const CELL = 11;
const GAP = 3;
/** The weekday labels' column. */
const GUTTER = 30;

function Activity({
  calls,
  color,
  onOpen,
}: {
  calls: Call[];
  color?: FolderColor;
  onOpen: (id: CallId) => void;
}) {
  // As many weeks as fit the width: the grid always runs edge to edge. Measured before
  // the first paint, so the grid never jumps.
  const ref = useRef<HTMLDivElement>(null);
  const [weeks, setWeeks] = useState(26);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const fit = (width: number) =>
      setWeeks(Math.max(8, Math.min(53, Math.floor((width - GUTTER + GAP) / (CELL + GAP)))));
    fit(el.clientWidth);
    const observer = new ResizeObserver(([entry]) => fit(entry.contentRect.width));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const days = useMemo(() => buildDays(calls, weeks), [calls, weeks]);
  const months = useMemo(() => monthLabels(days, weeks), [days, weeks]);
  const [hover, setHover] = useState<Day | null>(null);
  const activeDays = days.filter((d) => d.calls.length > 0).length;

  const reading = hover
    ? `${weekdayDate(hover.date)} · ${
        hover.calls.length === 0
          ? "sin llamadas"
          : [plural(hover.calls.length, "llamada", "llamadas"), hover.ms > 0 && formatDuration(hover.ms)]
              .filter(Boolean)
              .join(" · ")
      }`
    : `${plural(activeDays, "día", "días")} con llamadas · ${weeks} semanas`;

  return (
    <Section label="actividad" side={<span className="tech-label activity-reading">{reading}</span>}>
      <div className="activity" data-color={color} ref={ref}>
        <div
          className="activity-months tech-label"
          style={{ gridTemplateColumns: `repeat(${weeks}, ${CELL}px)`, columnGap: GAP, paddingLeft: GUTTER }}
        >
          {months.map((m) => (
            <span key={m.week} style={{ gridColumn: m.week + 1 }}>
              {m.label}
            </span>
          ))}
        </div>
        <div className="activity-body">
          <div className="activity-weekdays tech-label" style={{ width: GUTTER }}>
            {["lun", "", "mié", "", "vie", "", ""].map((d, i) => (
              <span key={i}>{d}</span>
            ))}
          </div>
          <div
            className="activity-grid"
            style={{ gridAutoColumns: CELL, gridTemplateRows: `repeat(7, ${CELL}px)`, gap: GAP }}
            onPointerLeave={() => setHover(null)}
          >
            {days.map((day) => {
              const props = {
                className: `day${day.today ? " today" : ""}`,
                "data-level": day.future ? undefined : level(day),
                "data-future": day.future || undefined,
                onPointerEnter: () => !day.future && setHover(day),
              };
              return day.calls.length > 0 ? (
                <button
                  key={day.date.getTime()}
                  {...props}
                  aria-label={`${weekdayDate(day.date)}: ${plural(day.calls.length, "llamada", "llamadas")}`}
                  onClick={() => onOpen(day.calls[0].id)}
                />
              ) : (
                <span key={day.date.getTime()} {...props} />
              );
            })}
          </div>
        </div>
        <div className="activity-legend tech-label" aria-hidden>
          menos
          {[0, 1, 2, 3, 4].map((l) => (
            <span key={l} className="day" data-level={l} />
          ))}
          más
        </div>
      </div>
    </Section>
  );
}

/** `weeks` columns of seven days, Monday first, ending with the current week. */
function buildDays(calls: Call[], weeks: number): Day[] {
  const today = startOfDay(new Date());
  const start = new Date(today);
  start.setDate(today.getDate() - ((today.getDay() + 6) % 7) - (weeks - 1) * 7);

  const byDay = new Map<number, Call[]>();
  for (const call of calls) {
    const key = startOfDay(new Date(call.created_at)).getTime();
    byDay.set(key, [...(byDay.get(key) ?? []), call]);
  }

  return Array.from({ length: weeks * 7 }, (_, i) => {
    const date = new Date(start);
    date.setDate(start.getDate() + i);
    const dayCalls = byDay.get(date.getTime()) ?? [];
    return {
      date,
      calls: dayCalls,
      ms: dayCalls.reduce((ms, c) => ms + (c.duration_ms ?? 0), 0),
      today: date.getTime() === today.getTime(),
      future: date > today,
    };
  });
}

/** By minutes on the call that day; a call of unknown length still lights its day. */
function level(day: Day): number {
  if (day.calls.length === 0) return 0;
  const minutes = day.ms / 60000;
  if (minutes < 20) return 1;
  if (minutes < 45) return 2;
  if (minutes < 90) return 3;
  return 4;
}

const monthFormat = new Intl.DateTimeFormat("es-AR", { month: "short" });

/** A month's name over the week its first day falls in, unless it would crowd the previous one. */
function monthLabels(days: Day[], weeks: number): { week: number; label: string }[] {
  const labels: { week: number; label: string }[] = [];
  for (let week = 0; week < weeks; week++) {
    const first = days.slice(week * 7, week * 7 + 7).find((d) => d.date.getDate() === 1);
    const date = first?.date ?? (week === 0 ? days[0].date : null);
    if (!date) continue;
    if (labels.length > 0 && week - labels[labels.length - 1].week < 3) continue;
    labels.push({ week, label: monthFormat.format(date).replace(".", "") });
  }
  // The first column's label goes if the next month's starts right after it.
  if (labels.length > 1 && labels[0].week === 0 && labels[1].week < 3) labels.shift();
  return labels;
}

// ── Recientes ───────────────────────────────────────────────────────────

function CallTile({
  index,
  call,
  summary,
  progress,
  folders,
  showFolder,
  onOpen,
}: {
  index: number;
  call: Call;
  summary?: Summary;
  progress?: ProgressMap[CallId];
  folders: Folder[];
  showFolder: boolean;
  onOpen: (id: CallId) => void;
}) {
  const color = folderColor(folders, call.folder);
  const decisions = summary?.decisions.length ?? 0;
  const todos = summary?.action_items.length ?? 0;
  const { status } = call;

  return (
    <button className="tile row-tick lane-in" style={lane(index)} onClick={() => onOpen(call.id)}>
      <span className="tile-rail tech-label">
        <span>{tileDate(call.created_at)}</span>
        {call.duration_ms != null && <span>{formatDuration(call.duration_ms)}</span>}
      </span>
      <span className="tile-title">{call.title}</span>
      <span className="tile-text" data-state={summary ? undefined : status.state}>
        {summary ? (
          summary.summary.split("\n").find(Boolean)
        ) : status.state === "processing" ? (
          <span className="mini-progress">
            <span className="mini-progress-label">
              {stepLabel(status.step)} {Math.round((progress?.fraction ?? 0) * 100)}%
            </span>
            <span className="bar">
              <span style={{ width: `${(progress?.fraction ?? 0) * 100}%` }} />
            </span>
          </span>
        ) : status.state === "recording" ? (
          <span className="tile-live">
            <span className="signal-square" />
            Grabando ahora.
          </span>
        ) : status.state === "pending" ? (
          "En cola para transcribir."
        ) : status.state === "failed" ? (
          `Falló: ${stepLabel(status.step).toLowerCase()}.`
        ) : (
          "Sin resumen."
        )}
      </span>
      {(decisions > 0 || todos > 0 || (showFolder && call.folder)) && (
        <span className="tile-foot">
          {showFolder && call.folder && (
            <span className="tile-fact" data-color={color}>
              <FolderMark color={color} />
              {call.folder}
            </span>
          )}
          {decisions > 0 && (
            <span className="tile-fact settled">
              <CheckCircleIcon />
              {decisions}
            </span>
          )}
          {todos > 0 && (
            <span className="tile-fact pending">
              <span className="todo-box" />
              {todos}
            </span>
          )}
        </span>
      )}
    </button>
  );
}

// ── Pendientes ──────────────────────────────────────────────────────────

function Todos({ todos, onOpen }: { todos: { call: Call; item: ActionItem }[]; onOpen: (id: CallId) => void }) {
  const [all, setAll] = useState(false);
  const shown = all ? todos : todos.slice(0, TODOS);
  return (
    <Section
      label="pendientes"
      count={todos.length}
      side={
        todos.length > TODOS && (
          <button className="ghost-button tech-label" onClick={() => setAll(!all)}>
            {all ? "ver menos" : `ver los ${todos.length}`}
          </button>
        )
      }
    >
      <ul className="todos dash-todos">
        {shown.map(({ call, item }, i) => (
          <li key={`${call.id}-${i}`} className="row-tick lane-in" style={lane(i)}>
            <span className="todo-box" />
            <div className="dash-todo-body">
              <p>{item.task}</p>
              {(item.owner || item.due) && (
                <div className="chips">
                  {item.owner && (
                    <span className="chip">
                      <PersonIcon />
                      {item.owner}
                    </span>
                  )}
                  {item.due && (
                    <span className="chip">
                      <CalendarIcon />
                      {item.due}
                    </span>
                  )}
                </div>
              )}
            </div>
            <button className="dash-todo-source" onClick={() => onOpen(call.id)} title="Abrir la llamada">
              {call.title}
            </button>
          </li>
        ))}
      </ul>
    </Section>
  );
}

// ── Carpetas ────────────────────────────────────────────────────────────

function FolderBreakdown({
  calls,
  folders,
  onScope,
}: {
  calls: Call[];
  folders: Folder[];
  onScope: (scope: Scope) => void;
}) {
  type Row = { scope: Scope; name: string; color?: FolderColor; calls: Call[] };
  const rows = [
    ...folders.map(
      (f): Row => ({
        scope: { kind: "folder", name: f.name },
        name: f.name,
        color: f.color,
        calls: calls.filter((c) => c.folder === f.name),
      }),
    ),
    { scope: { kind: "unfiled" }, name: "Sin carpeta", calls: calls.filter((c) => !c.folder) } satisfies Row,
  ].filter((r) => r.calls.length > 0 || r.scope.kind === "folder");
  const most = Math.max(1, ...rows.map((r) => r.calls.length));

  return (
    <Section label="carpetas" count={folders.length}>
      <ul className="folder-rows">
        {rows.map((row, i) => {
          const ms = row.calls.reduce((n, c) => n + (c.duration_ms ?? 0), 0);
          return (
            <li key={row.name} className="lane-in" style={lane(i)}>
              <button
                className="folder-row row-tick"
                data-color={row.color}
                data-unfiled={row.scope.kind === "unfiled" || undefined}
                onClick={() => onScope(row.scope)}
              >
                <FolderMark color={row.color} />
                <span className="folder-row-name">{row.name}</span>
                <span className="folder-row-bar">
                  <span style={{ width: `${(row.calls.length / most) * 100}%` }} />
                </span>
                <span className="folder-row-count">{row.calls.length}</span>
                <span className="folder-row-time">{ms > 0 ? formatDuration(ms) : "—"}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </Section>
  );
}

// ── Dates ───────────────────────────────────────────────────────────────

function startOfDay(date: Date): Date {
  const day = new Date(date);
  day.setHours(0, 0, 0, 0);
  return day;
}

function daysAgo(iso: string): number {
  const day = 24 * 60 * 60 * 1000;
  return Math.round((startOfDay(new Date()).getTime() - startOfDay(new Date(iso)).getTime()) / day);
}

const shortDate = new Intl.DateTimeFormat("es-AR", { day: "numeric", month: "short" });
const weekdayFormat = new Intl.DateTimeFormat("es-AR", { weekday: "short", day: "numeric", month: "short" });
const timeFormat = new Intl.DateTimeFormat("es-AR", { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });

const weekdayDate = (date: Date) => weekdayFormat.format(date).replace(",", "").replace(/\./g, "");

/** "hoy", "ayer", "hace 3 días", "hace 2 semanas", then the date. */
function ago(iso: string): string {
  const days = daysAgo(iso);
  if (days <= 0) return "hoy";
  if (days === 1) return "ayer";
  if (days < 7) return `hace ${days} días`;
  if (days < 30) {
    const weeks = Math.round(days / 7);
    return weeks === 1 ? "hace una semana" : `hace ${weeks} semanas`;
  }
  return shortDate.format(new Date(iso)).replace(".", "");
}

/** "hoy · 14:30", "ayer · 09:10", "mié 22 oct · 14:30". */
function tileDate(iso: string): string {
  const date = new Date(iso);
  const days = daysAgo(iso);
  const day = days <= 0 ? "hoy" : days === 1 ? "ayer" : weekdayDate(date);
  return `${day} · ${timeFormat.format(date)}`;
}
