// Typed bridge to the Rust side. Types mirror the serde output of `calls-core`
// (crates/core) and the commands in `src-tauri/src/commands.rs`: keep them in sync.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type CallId = string;

export type Status =
  | { state: "recording" }
  | { state: "pending" }
  | { state: "processing"; step: string }
  | { state: "ready" }
  | { state: "failed"; step: string; error: string };

export interface Track {
  file: string;
  speaker: string | null;
  role: "mixed" | "microphone" | "system";
}

export interface Call {
  id: CallId;
  title: string;
  folder?: string;
  created_at: string;
  source: string | null;
  duration_ms: number | null;
  tracks: Track[];
  status: Status;
  completed_steps: Record<string, string>;
}

export interface Segment {
  start_ms: number;
  end_ms: number;
  text: string;
  speaker?: string;
}

export interface Transcript {
  language: string;
  segments: Segment[];
}

export interface ActionItem {
  task: string;
  owner: string | null;
  due: string | null;
}

export interface Summary {
  summary: string;
  decisions: string[];
  action_items: ActionItem[];
}

export interface CallDetail {
  call: Call;
  dir: string;
  transcript: Transcript | null;
  summary: Summary | null;
}

/** A folder's tint; mirrors `FolderColor` in crates/core/src/library.rs. */
export type FolderColor = "red" | "yellow" | "green" | "blue";

export const FOLDER_COLORS: Record<FolderColor, string> = {
  red: "Rojo",
  yellow: "Amarillo",
  green: "Verde",
  blue: "Azul",
};

export interface Folder {
  name: string;
  color?: FolderColor;
}

export interface Doctor {
  ffmpeg: string | null;
  whisper_cli: string | null;
  calls_audio: string | null;
  whisper_model: string;
  whisper_model_present: boolean;
  vad_model_present: boolean;
  library_dir: string;
  uv: string | null;
  llm_error: string | null;
}

export type PipelineEvent =
  | { kind: "step_started"; call_id: CallId; step: string }
  | { kind: "step_progress"; call_id: CallId; step: string; fraction: number }
  | { kind: "step_finished"; call_id: CallId; step: string }
  | { kind: "finished"; call_id: CallId }
  | { kind: "failed"; call_id: CallId; step: string; error: string };

export const api = {
  listCalls: () => invoke<Call[]>("list_calls"),
  getCall: (id: CallId) => invoke<CallDetail>("get_call", { id }),
  importFiles: (paths: string[]) => invoke<Call[]>("import_files", { paths }),
  processCall: (id: CallId) => invoke<void>("process_call", { id }),
  renameCall: (id: CallId, title: string) => invoke<Call>("rename_call", { id, title }),
  /** `null` takes the call out of any folder; a new name creates the folder. */
  moveCall: (id: CallId, folder: string | null) => invoke<Call>("move_call", { id, folder }),
  listFolders: () => invoke<Folder[]>("list_folders"),
  createFolder: (name: string) => invoke<string>("create_folder", { name }),
  renameFolder: (from: string, to: string) => invoke<void>("rename_folder", { from, to }),
  deleteFolder: (name: string) => invoke<void>("delete_folder", { name }),
  /** `null` clears the color. */
  setFolderColor: (name: string, color: FolderColor | null) => invoke<void>("set_folder_color", { name, color }),
  doctor: () => invoke<Doctor>("doctor"),
  islandState: () => invoke<Island>("island_state"),
  startRecording: () => invoke<CallId>("start_recording"),
  stopRecording: () => invoke<void>("stop_recording"),
  dismissIsland: () => invoke<void>("dismiss_island"),
  showCall: (id: CallId) => invoke<void>("show_call", { id }),
};

/** What the island shows; mirrors `Island` in src-tauri/src/recorder.rs. */
export type Island =
  | { kind: "hidden" }
  | { kind: "detected"; app_name: string }
  | { kind: "recording"; call_id: CallId; app_name: string | null; started_at_ms: number }
  | { kind: "processing"; call_id: CallId; step: string; fraction: number }
  | { kind: "done"; call_id: CallId; failed: boolean };

export interface Levels {
  mic: number;
  system: number;
}

export const onIsland = (handler: (island: Island) => void) => listen<Island>("island", (e) => handler(e.payload));
export const onLevels = (handler: (levels: Levels) => void) =>
  listen<Levels>("audio_levels", (e) => handler(e.payload));
export const onLibraryChanged = (handler: () => void) => listen("library_changed", () => handler());
export const onSelectCall = (handler: (id: CallId) => void) => listen<CallId>("select_call", (e) => handler(e.payload));

export function onPipelineEvent(handler: (event: PipelineEvent) => void): Promise<UnlistenFn> {
  return listen<PipelineEvent>("pipeline", (e) => handler(e.payload));
}

// Labels for step ids. Steps without a label show their id.
export const STEP_LABELS: Record<string, string> = {
  setup: "Preparando",
  saving: "Guardando grabación",
  extract_audio: "Extrayendo audio",
  transcribe: "Transcribiendo",
  render_markdown: "Generando documento",
  summarize: "Resumiendo",
};

export const MEDIA_EXTENSIONS = ["mp4", "mkv", "mov", "webm", "m4a", "mp3", "wav", "aac", "flac", "ogg"];
