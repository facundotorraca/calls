import { useEffect, useState } from "react";
import { api, type Doctor, type LocalModel, type Settings } from "./api";
import { DownloadControl, downloadKey, useDownloads } from "./downloads";

/** The one thing a new install needs before it can transcribe, a click away. Everything else
 * is optional and waits in the settings until it is asked for. */
export function SetupBanner({ doctor, settings }: { doctor: Doctor; settings: Settings }) {
  const downloads = useDownloads();
  const [model, setModel] = useState<LocalModel | null>(null);
  const name = settings.whisper_model;

  useEffect(() => {
    api.localModels("whisper").then((models) => setModel(models.find((m) => m.name === name) ?? null));
  }, [name, downloads.finished]);

  // Bundled with the app: only a build from source can be missing them.
  const sidecars = [
    !doctor.ffmpeg && "ffmpeg",
    !doctor.whisper_cli && "whisper-cli",
    !doctor.calls_audio && "calls-audio",
  ].filter(Boolean);

  if (sidecars.length > 0) {
    return (
      <div className="banner">
        <div className="banner-rail tech-label">
          <span>falta compilar</span>
          <code>make sidecars</code>
        </div>
        <div className="banner-body muted">{sidecars.join(", ")}</div>
      </div>
    );
  }
  if (doctor.whisper_model_present) return null;

  const target = { kind: "whisper", name } as const;
  const error = downloads.errors[downloadKey(target)];
  return (
    <div className="banner">
      <div className="banner-rail tech-label">
        <span>un paso más</span>
        <button className="ghost-button tech-label" onClick={() => api.openSettings("pipeline")}>
          elegir otro modelo
        </button>
      </div>
      <div className="banner-body setup-now">
        <p className="muted">Para transcribir falta el modelo: {name}. Se descarga una sola vez.</p>
        <DownloadControl
          progress={downloads.progress[downloadKey(target)]}
          bytes={model?.bytes}
          onDownload={() => downloads.start(target)}
          onCancel={() => downloads.cancel(target)}
        />
      </div>
      {error && <p className="model-error setup-now-error">{error}</p>}
    </div>
  );
}
