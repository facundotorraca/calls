import { useEffect, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type Call, type CallDetail, type Transcript } from "./api";
import type { ProgressMap } from "./CallList";
import { formatDate, formatDuration, formatTimestamp, stepLabel } from "./format";

interface Props {
  call: Call;
  progress?: ProgressMap[string];
}

export function CallView({ call, progress }: Props) {
  const [detail, setDetail] = useState<CallDetail | null>(null);
  const [copied, setCopied] = useState(false);

  // Reload when switching calls or when processing moves it to a new state.
  useEffect(() => {
    let cancelled = false;
    api.getCall(call.id).then((d) => !cancelled && setDetail(d));
    return () => {
      cancelled = true;
    };
  }, [call.id, call.status.state]);

  const transcript = detail?.call.id === call.id ? detail.transcript : null;

  const copy = async () => {
    if (!transcript) return;
    await navigator.clipboard.writeText(toPlainText(transcript));
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <article className="call-view">
      <header className="call-header">
        <div>
          <h2>{call.title}</h2>
          <p className="muted">
            {formatDate(call.created_at)}
            {call.duration_ms != null && ` · ${formatDuration(call.duration_ms)}`}
            {transcript?.language && ` · ${transcript.language.toUpperCase()}`}
          </p>
        </div>
        <div className="actions">
          {transcript && (
            <button className="button" onClick={copy}>
              {copied ? "¡Copiado!" : "Copiar texto"}
            </button>
          )}
          {detail && (
            <button className="button" onClick={() => revealItemInDir(detail.dir)}>
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

      {call.status.state === "recording" && (
        <div className="processing">
          <p>Grabando… la transcripción empieza al terminar la llamada.</p>
        </div>
      )}

      {(call.status.state === "pending" || call.status.state === "processing") && (
        <div className="processing">
          <p>{call.status.state === "processing" ? stepLabel(call.status.step) : "En cola"}…</p>
          <div className="bar large">
            <span style={{ width: `${(progress?.fraction ?? 0) * 100}%` }} />
          </div>
        </div>
      )}

      {transcript && (
        <ol className="transcript">
          {transcript.segments.map((segment, i) => (
            <li key={i} className="segment">
              <span className="timestamp">{formatTimestamp(segment.start_ms)}</span>
              <p>
                {segment.speaker && <strong className="speaker">{segment.speaker}: </strong>}
                {segment.text}
              </p>
            </li>
          ))}
        </ol>
      )}
    </article>
  );
}

function toPlainText(transcript: Transcript): string {
  return transcript.segments
    .map((s) => `[${formatTimestamp(s.start_ms)}] ${s.speaker ? `${s.speaker}: ` : ""}${s.text}`)
    .join("\n");
}
