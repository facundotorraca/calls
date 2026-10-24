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
    return doctor.llm_error ? <div className="banner warning">Resúmenes no disponibles: {doctor.llm_error}</div> : null;
  }

  return (
    <div className="banner warning">
      <strong>Falta configurar algunas cosas para poder transcribir:</strong>
      <ul>
        {missing.map(([what, fix]) => (
          <li key={what}>
            {what}: <code>{fix}</code>
          </li>
        ))}
      </ul>
    </div>
  );
}
