// Model downloads as the windows see them. They run in the app, not in a window: closing it
// does not stop them.
import { useCallback, useEffect, useState } from "react";
import { api, onDownload, type DownloadTarget } from "./api";
import { formatBytes } from "./format";

export const downloadKey = (t: DownloadTarget) => `${t.kind}:${t.name}`;

export interface Downloads {
  /** Fraction done of each running download, by `kind:name`. */
  progress: Record<string, number>;
  /** Why the last download of each failed. */
  errors: Record<string, string>;
  /** Bumped when a download finishes, to reload the lists. */
  finished: number;
  start: (target: DownloadTarget, token?: string) => void;
  cancel: (target: DownloadTarget) => void;
}

export function useDownloads(): Downloads {
  const [progress, setProgress] = useState<Record<string, number>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [finished, setFinished] = useState(0);

  useEffect(() => {
    api.downloads().then((running) => setProgress(Object.fromEntries(running.map((d) => [downloadKey(d), d.fraction]))));
    const unlisten = onDownload((event) => {
      const k = downloadKey(event);
      if (event.state === "progress") {
        setProgress((p) => ({ ...p, [k]: event.fraction }));
        return;
      }
      setProgress(({ [k]: _, ...rest }) => rest);
      if (event.state === "failed") setErrors((e) => ({ ...e, [k]: event.error }));
      setFinished((n) => n + 1);
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

  const start = useCallback((target: DownloadTarget, token?: string) => {
    const k = downloadKey(target);
    setErrors(({ [k]: _, ...rest }) => rest);
    setProgress((p) => ({ ...p, [k]: 0 }));
    api.startDownload(target, token);
  }, []);

  return { progress, errors, finished, start, cancel: api.cancelDownload };
}

export function DownloadControl({
  progress,
  bytes,
  onDownload,
  onCancel,
}: {
  progress?: number;
  bytes?: number | null;
  onDownload?: () => void;
  onCancel?: () => void;
}) {
  if (progress == null) {
    return (
      <button className="button" onClick={onDownload}>
        descargar{bytes != null && ` · ${formatBytes(bytes)}`}
      </button>
    );
  }
  return (
    <span className="download">
      <span className="bar">
        <span style={{ width: `${progress * 100}%` }} />
      </span>
      <span className="download-pct tech-label">{Math.floor(progress * 100)}%</span>
      {onCancel && (
        <button className="ghost-button tech-label" onClick={onCancel}>
          cancelar
        </button>
      )}
    </span>
  );
}
