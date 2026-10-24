import type { Doctor } from "./api";

/** Tells the user exactly what is missing and how to fix it. */
export function SetupBanner({ doctor }: { doctor: Doctor }) {
  const missing: [string, string][] = [];
  if (!doctor.ffmpeg) missing.push(["ffmpeg", "brew install ffmpeg"]);
  if (!doctor.whisper_cli) missing.push(["whisper-cli", "make sidecars"]);
  if (!doctor.whisper_model_present) missing.push(["el modelo de Whisper", "make models"]);
  if (!doctor.calls_audio) missing.push(["calls-audio (grabar llamadas)", "make sidecars"]);
  if (missing.length === 0) {
    // Not blocking: transcripts work without the LLM.
    return doctor.llm_error ? (
      <div className="banner">
        <div className="banner-rail tech-label">
          <span>resúmenes no disponibles</span>
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
              <code>{fix}</code>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
