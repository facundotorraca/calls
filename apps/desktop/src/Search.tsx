import { useEffect, useState, type CSSProperties } from "react";
import { api, type Call, type CallId, type Folder, type Hit, type HitKind, type SearchResult } from "./api";
import { folderColor, FolderMark } from "./Folders";
import { formatDate, formatDuration, formatTimestamp } from "./format";
import { plural } from "./SummaryView";

/** What opening a call from a search shows: a tab, and in it a moment. */
export interface Focus {
  tab: "summary" | "notes" | "transcript";
  at_ms?: number;
  /** A new object each time, so the same place can be asked for twice. */
  nonce: number;
}

/**
 * Lowercase without accents, one UTF-16 unit for each one of `text`, so positions in the
 * folded text are positions in the original. Same folding as crates/core/src/search.rs.
 */
export function fold(text: string): string {
  let out = "";
  for (let i = 0; i < text.length; i++) out += text[i].normalize("NFD")[0].toLowerCase()[0];
  return out;
}

/** The query's words, folded, without repeats. */
export const queryWords = (query: string) => [...new Set(fold(query).split(/\s+/).filter(Boolean))];

/** Where the words are in `text`, as merged [start, end) ranges. */
function ranges(text: string, words: string[]): [number, number][] {
  const folded = fold(text);
  const found: [number, number][] = [];
  for (const word of words) {
    for (let i = folded.indexOf(word); i !== -1; i = folded.indexOf(word, i + word.length)) {
      found.push([i, i + word.length]);
    }
  }
  found.sort((a, b) => a[0] - b[0]);
  const merged: [number, number][] = [];
  for (const range of found) {
    const last = merged[merged.length - 1];
    if (last && range[0] <= last[1]) last[1] = Math.max(last[1], range[1]);
    else merged.push(range);
  }
  return merged;
}

/** `text` with the words of a search marked. */
export function Highlight({ text, words }: { text: string; words?: string[] }) {
  if (!words?.length) return <>{text}</>;
  const parts = [];
  let at = 0;
  for (const [start, end] of ranges(text, words)) {
    if (start > at) parts.push(text.slice(at, start));
    parts.push(
      <mark key={start} className="hl">
        {text.slice(start, end)}
      </mark>,
    );
    at = end;
  }
  parts.push(text.slice(at));
  return <>{parts}</>;
}

/** A long passage cut down to the stretch around its first match. */
function excerpt(text: string, words: string[], radius = 110): string {
  if (text.length <= radius * 2) return text;
  const first = ranges(text, words)[0]?.[0] ?? 0;
  let start = Math.max(0, first - radius / 2);
  let end = Math.min(text.length, start + radius * 2);
  start = Math.max(0, end - radius * 2);
  // Whole words at the edges.
  if (start > 0) start = text.indexOf(" ", start) + 1 || start;
  if (end < text.length) end = text.lastIndexOf(" ", end) > start ? text.lastIndexOf(" ", end) : end;
  return `${start > 0 ? "…" : ""}${text.slice(start, end)}${end < text.length ? "…" : ""}`;
}

const KIND_LABELS: Record<HitKind, string> = {
  summary: "resumen",
  decision: "decisión",
  task: "pendiente",
  notes: "nota",
  mark: "momento",
  transcript: "transcripción",
};

const KIND_TABS: Record<HitKind, Focus["tab"]> = {
  summary: "summary",
  decision: "summary",
  task: "summary",
  notes: "notes",
  mark: "notes",
  transcript: "transcript",
};

/** The search, run on the Rust side as you type; stale answers are dropped. */
export function useSearch(query: string): { results: SearchResult[] | null; pending: boolean } {
  const [state, setState] = useState<{ query: string; results: SearchResult[] } | null>(null);
  const trimmed = query.trim();
  useEffect(() => {
    if (!trimmed) {
      setState(null);
      return;
    }
    let cancelled = false;
    // Typing fast asks once, when the hands stop.
    const timer = setTimeout(() => {
      api.search(trimmed).then(
        (results) => !cancelled && setState({ query: trimmed, results }),
        () => !cancelled && setState({ query: trimmed, results: [] }),
      );
    }, 160);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [trimmed]);
  return { results: trimmed ? (state?.results ?? null) : null, pending: !!trimmed && state?.query !== trimmed };
}

interface Props {
  query: string;
  results: SearchResult[] | null;
  pending: boolean;
  calls: Call[];
  folders: Folder[];
  onOpen: (id: CallId, focus?: Focus) => void;
}

/** The page a search shows: each call found, with the passages where the words are. */
export function SearchResults({ query, results, pending, calls, folders, onOpen }: Props) {
  const words = queryWords(query);
  const byId = new Map(calls.map((c) => [c.id, c]));
  const found = (results ?? []).flatMap((r) => {
    const call = byId.get(r.id);
    return call ? [{ call, result: r }] : [];
  });
  const passages = found.reduce((n, f) => n + f.result.total, 0);

  return (
    <article className="dash">
      <header className="dash-header dot-grid">
        <div className="dash-rail tech-label">
          <span className="dash-crumbs">
            calls<span className="dash-crumb-sep">/</span>
            <span className="dash-crumb-here">búsqueda</span>
          </span>
          <span className="search-state">
            {pending && <span className="spinner" />}
            {results &&
              (found.length > 0
                ? `${plural(passages, "pasaje", "pasajes")} · ${plural(found.length, "llamada", "llamadas")}`
                : "sin resultados")}
          </span>
        </div>
        <div className="dash-title-row">
          <h2 className="dash-title search-title">“{query.trim()}”</h2>
        </div>
      </header>

      {results && found.length === 0 ? (
        <section className="empty dash-empty">
          <p className="empty-label tech-label">sin resultados</p>
          <p className="empty-title">Ninguna llamada dice “{query.trim()}”.</p>
          <p className="empty-hint">
            Se busca en títulos, carpetas, resúmenes, notas y transcripciones, sin importar tildes ni mayúsculas.
            Una llamada aparece si tiene todas las palabras.
          </p>
        </section>
      ) : (
        <div className="dash-body search-body">
          {found.map(({ call, result }, i) => (
            <CallResult
              key={call.id}
              index={i}
              call={call}
              result={result}
              words={words}
              folders={folders}
              onOpen={onOpen}
            />
          ))}
        </div>
      )}
    </article>
  );
}

function CallResult({
  index,
  call,
  result,
  words,
  folders,
  onOpen,
}: {
  index: number;
  call: Call;
  result: SearchResult;
  words: string[];
  folders: Folder[];
  onOpen: (id: CallId, focus?: Focus) => void;
}) {
  const color = folderColor(folders, call.folder);
  const more = result.total - result.hits.length;
  const open = (hit: Hit) => onOpen(call.id, { tab: KIND_TABS[hit.kind], at_ms: hit.at_ms, nonce: Date.now() });

  return (
    <section className="result lane-in" style={{ "--lane-index": Math.min(index, 12) } as CSSProperties}>
      <button className="result-head" onClick={() => onOpen(call.id)}>
        <span className="result-rail tech-label">
          <span>{formatDate(call.created_at)}</span>
          {call.duration_ms != null && <span>{formatDuration(call.duration_ms)}</span>}
          {call.folder && (
            <span className="result-folder" data-color={color}>
              <FolderMark color={color} />
              {call.folder}
            </span>
          )}
        </span>
        <span className="result-title">
          <Highlight text={call.title} words={words} />
        </span>
      </button>

      {result.hits.length > 0 && (
        <ul className="hits">
          {result.hits.map((hit, i) => (
            <li key={i}>
              <button className="hit row-tick" onClick={() => open(hit)}>
                <span className="hit-kind tech-label" data-kind={hit.kind}>
                  {KIND_LABELS[hit.kind]}
                  {hit.at_ms != null && <span className="hit-time">{formatTimestamp(hit.at_ms)}</span>}
                </span>
                <span className="hit-text">
                  {hit.speaker && <span className="hit-speaker">{hit.speaker}</span>}
                  {hit.text ? (
                    <Highlight text={excerpt(hit.text, words)} words={words} />
                  ) : (
                    <span className="hit-empty">momento marcado</span>
                  )}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
      {more > 0 && (
        <button
          className="ghost-button tech-label result-more"
          onClick={() => onOpen(call.id, { tab: "transcript", nonce: Date.now() })}
        >
          {plural(more, "pasaje más", "pasajes más")}
        </button>
      )}
    </section>
  );
}
