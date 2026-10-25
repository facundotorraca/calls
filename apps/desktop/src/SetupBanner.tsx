import type { ReactNode } from "react";
import { api, type Doctor, type SettingsPage } from "./api";

/** Tells the user exactly what is missing and how to fix it. */
export function SetupBanner({ doctor }: { doctor: Doctor }) {
  const settingsButton = (page: SettingsPage) => (
    <button className="ghost-button tech-label" onClick={() => api.openSettings(page)}>
      ajustes
    </button>
  );

  const missing: [string, ReactNode][] = [];
  if (!doctor.ffmpeg) missing.push(["ffmpeg", <code>brew install ffmpeg</code>]);
  if (!doctor.whisper_cli) missing.push(["whisper-cli", <code>make sidecars</code>]);
  if (!doctor.whisper_model_present) missing.push(["el modelo de Whisper", settingsButton("transcription")]);
  if (!doctor.calls_audio) missing.push(["calls-audio (grabar llamadas)", <code>make sidecars</code>]);
  if (missing.length === 0) {
    // Not blocking: transcripts work without the LLM.
    return doctor.llm_error ? (
      <div className="banner">
        <div className="banner-rail tech-label">
          <span>resúmenes no disponibles</span>
          {settingsButton("summary")}
        </div>
        <div className="banner-body muted">{doctor.llm_error}</div>
      </div>
    ) : null;
  }

  return (
    <div className="banner">
      <div className="banner-rail tech-label">
        <span>configuración</span>
        <span>
          {missing.length} pendiente{missing.length === 1 ? "" : "s"}
        </span>
      </div>
      <div className="banner-body">
        <p className="muted">Falta configurar algunas cosas para poder transcribir:</p>
        <ul>
          {missing.map(([what, fix]) => (
            <li key={what}>
              {what}
              {fix}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
