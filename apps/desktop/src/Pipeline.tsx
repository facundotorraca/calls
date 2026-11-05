// What happens to a call once it is recorded, drawn as a line of stages: which ones ran, which
// one is running, which are off and which are waiting for something. From it, any of them can
// be run again.
import { ask } from "@tauri-apps/plugin-dialog";
import { api, type Call, type Doctor, type Settings } from "./api";
import type { ProgressMap } from "./CallList";
import { RepeatIcon } from "./icons";
import { popupMenu } from "./ui";

/** A stage is what the user thinks of; the pipeline's steps are how it gets done. */
interface Stage {
  id: "audio" | "silences" | "transcribe" | "voices" | "summary";
  label: string;
  steps: string[];
  /** The step whose being done means the stage is. */
  main: string;
  /** The step to run it again from, if it can be. */
  from?: Rerun;
}

/** The steps a call can be reprocessed from; the ones after them run again too. */
type Rerun = "transcribe" | "diarize" | "summarize";

const STAGES: Stage[] = [
  { id: "audio", label: "audio", steps: ["extract_audio", "playback", "recording", "setup"], main: "extract_audio" },
  // Not a step of its own: whisper skips them as it transcribes (`SILENCES` in steps.rs).
  { id: "silences", label: "silencios", steps: [], main: "silences", from: "transcribe" },
  { id: "transcribe", label: "transcribir", steps: ["transcribe", "render_markdown"], main: "transcribe", from: "transcribe" },
  { id: "voices", label: "voces", steps: ["diarize"], main: "diarize", from: "diarize" },
  { id: "summary", label: "resumen", steps: ["name_speakers", "summarize"], main: "summarize", from: "summarize" },
];

type StageState =
  | { kind: "done" | "queued" | "pending" | "off" | "unknown" | "setup" | "failed"; note: string }
  | { kind: "running"; note: string; fraction: number | null };

/** What is missing for a stage to run here, in a couple of words. `null` if it is ready. */
function missing(stage: Stage["id"], doctor: Doctor): string | null {
  switch (stage) {
    case "silences":
      return doctor.vad_model_present ? null : "falta modelo";
    case "transcribe":
      return doctor.whisper_model_present && doctor.whisper_cli ? null : "falta modelo";
    case "voices":
      return doctor.diarization ? null : "falta modelo";
    case "summary":
      if (!doctor.llm_error) return null;
      return doctor.llm_error.includes("not running") ? "sin ollama" : "falta modelo";
    case "audio":
      return doctor.ffmpeg ? null : "falta ffmpeg";
  }
}

const isOn = (stage: Stage["id"], settings: Settings) =>
  stage === "silences" ? settings.vad_model != null : stage === "voices" ? settings.diarize : stage === "summary" ? settings.summarize : true;

function stateOf(
  stage: Stage,
  call: Call,
  settings: Settings,
  doctor: Doctor,
  progress: ProgressMap[string] | undefined,
): StageState {
  const { status, completed_steps: done, step_errors: errors } = call;
  if (status.state === "processing") {
    if (stage.steps.includes(status.step)) {
      const fraction = progress?.step === status.step ? progress.fraction : null;
      return { kind: "running", note: fraction ? `${Math.round(fraction * 100)}%` : "en curso", fraction };
    }
    if (stage.id === "silences" && status.step === "transcribe" && isOn("silences", settings) && doctor.vad_model_present) {
      return { kind: "running", note: "en curso", fraction: null };
    }
  }
  if (status.state === "failed" && stage.steps.includes(status.step)) return { kind: "failed", note: "falló" };
  if (stage.steps.some((step) => errors?.[step])) return { kind: "failed", note: "falló" };
  if (stage.main in done) return { kind: "done", note: "listo" };
  // Transcribed before the app kept track of the silences, or with them off.
  if (stage.id === "silences" && "transcribe" in done) return { kind: "unknown", note: "—" };
  if (!isOn(stage.id, settings)) return { kind: "off", note: "apagado" };
  const needs = missing(stage.id, doctor);
  if (needs) return { kind: "setup", note: needs };
  if (status.state === "ready" || status.state === "failed") return { kind: "pending", note: "sin correr" };
  return { kind: "queued", note: "en cola" };
}

const RERUNS: Record<Rerun, { title: string; detail: string; menu: string; action: string }> = {
  transcribe: {
    title: "¿Volver a transcribir?",
    detail:
      "Se hace de nuevo con los ajustes de ahora, y también lo que sigue. Se pierden los nombres que hayas corregido a mano. Tus notas y el audio no se tocan.",
    menu: "Desde la transcripción",
    action: "Volver a transcribir",
  },
  diarize: {
    title: "¿Volver a separar las voces?",
    detail:
      "Se hace de nuevo con los ajustes de ahora, y también el resumen. Se pierden los nombres que hayas corregido a mano. Tus notas no se tocan.",
    menu: "Desde las voces",
    action: "Volver a separar las voces",
  },
  summarize: {
    title: "¿Volver a resumir?",
    detail: "Se reemplaza el resumen por uno nuevo, con los ajustes de ahora.",
    menu: "Solo el resumen",
    action: "Volver a resumir",
  },
};

interface Props {
  call: Call;
  settings: Settings;
  doctor: Doctor;
  progress?: ProgressMap[string];
  /** Tells the app the call went back in line, in case another one is ahead of it. */
  onChanged: () => Promise<void>;
  onError: (error: unknown) => void;
}

export function PipelineStrip({ call, settings, doctor, progress, onChanged, onError }: Props) {
  const state = call.status.state;
  const busy = state === "pending" || state === "processing";
  const run = () => api.processCall(call.id).then(onChanged).catch(onError);

  const rerun = async (from: Rerun) => {
    const { title, detail } = RERUNS[from];
    if (await ask(detail, { title, kind: "warning", okLabel: "Reprocesar", cancelLabel: "Cancelar" })) {
      api.reprocessCall(call.id, from).then(onChanged).catch(onError);
    }
  };

  const canRerun = (from: Rerun) => {
    if (from === "transcribe") return !missing("transcribe", doctor);
    const stage = from === "diarize" ? "voices" : "summary";
    return "transcribe" in call.completed_steps && isOn(stage, settings) && !missing(stage, doctor);
  };

  /** What clicking on a stage does, and what it says it does. Nothing while the call is at it. */
  const actionOf = (stage: Stage, stageState: StageState): [() => void, string] | null => {
    if (busy) return null;
    switch (stageState.kind) {
      case "off":
      case "setup":
        return [() => void api.openSettings("pipeline"), "Abrir ajustes"];
      case "pending":
        // Everything but the audio works on the transcript: without a way to get it, nothing to run.
        if (stage.id !== "audio" && !("transcribe" in call.completed_steps) && missing("transcribe", doctor)) return null;
        return [run, "Correr lo que falta"];
      case "failed":
        return [run, "Reintentar"];
      case "done":
      case "unknown":
        return stage.from && canRerun(stage.from) ? [() => void rerun(stage.from!), RERUNS[stage.from].action] : null;
      default:
        return null;
    }
  };

  return (
    <div className="pipeline">
      <ol className="pipeline-track">
        {STAGES.map((stage) => {
          const stageState = stateOf(stage, call, settings, doctor, progress);
          const action = actionOf(stage, stageState);
          const fraction = stageState.kind === "running" ? stageState.fraction : null;
          return (
            <li key={stage.id} className="stage" data-state={stageState.kind}>
              <button className="stage-button" disabled={!action} onClick={action?.[0]} title={action?.[1]}>
                <span className="stage-line">
                  <span className="stage-mark" />
                  <span className="stage-link">{fraction != null && <span style={{ width: `${fraction * 100}%` }} />}</span>
                </span>
                <span className="stage-label tech-label">{stage.label}</span>
                <span className="stage-note">{stageState.note}</span>
              </button>
            </li>
          );
        })}
      </ol>
      <button
        className="ghost-button tech-label pipeline-again"
        disabled={busy}
        onClick={() =>
          popupMenu(
            (Object.keys(RERUNS) as Rerun[]).map((from) => ({
              text: RERUNS[from].menu,
              enabled: canRerun(from),
              action: () => void rerun(from),
            })),
          )
        }
      >
        <RepeatIcon />
        reprocesar
      </button>
    </div>
  );
}
