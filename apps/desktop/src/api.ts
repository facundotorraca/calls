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
  /** The Notion page its calls go under. */
  notion?: NotionPage;
}

/** A Notion page picked by pasting its link (crates/core/src/notion.rs). */
export interface NotionPage {
  id: string;
  title: string;
  url: string;
}

/** Which level a call's Notion parent page was set at, from the most general. */
export type NotionLevel = "default" | "folder" | "call";

export interface NotionStatus {
  /** The page its page goes under, if any level has one. */
  destination: { page: NotionPage; level: NotionLevel } | null;
  /** Set for this call only. */
  call_parent: NotionPage | null;
  /** The last sync. */
  synced: { page_id: string; url: string; parent_id: string; at: string } | null;
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
  /** Voices get told apart: turned on and the model downloaded. */
  diarization: boolean;
}

/** Mirrors `Settings` in crates/core/src/settings.rs. Sent back whole when saving. */
export interface Settings {
  /** How the person using the app is labelled in transcripts. Empty: "Yo". */
  user_name: string;
  library_dir: string;
  /** ISO 639-1 code, or `auto`. */
  language: string;
  whisper_model: string;
  /** Names and terms, spelled right, that steer the transcriber and the summary. */
  vocabulary: string[];
  /** Tell the voices of a call apart (pyannote, downloaded once). */
  diarize: boolean;
  vad_model: string | null;
  detect_calls: boolean;
  auto_record: boolean;
  call_apps: unknown[];
  summarize: boolean;
  /** Written into the prompt, e.g. `español`. */
  summary_language: string;
  llm: { base_url: string; model: string };
  notion: {
    /** The connected workspace; the token is in the Keychain. */
    workspace: string | null;
    /** Where call pages go when neither the call nor its folder has a page of its own. */
    parent: NotionPage | null;
  };
}

/** A whisper.cpp model offered or on disk (crates/engines/src/models.rs). */
export interface LocalModel {
  name: string;
  note: string | null;
  /** On disk if downloaded, otherwise the size of the download. */
  bytes: number | null;
  downloaded: boolean;
}

export interface OllamaModel {
  name: string;
  size: number;
  details: { parameter_size: string; quantization_level: string };
}

export interface DownloadTarget {
  kind: "whisper" | "vad" | "ollama" | "diarization";
  name: string;
}

export type DownloadEvent = DownloadTarget &
  (
    | { state: "progress"; fraction: number }
    | { state: "done" }
    | { state: "failed"; error: string }
    | { state: "cancelled" }
  );

export type PipelineEvent =
  | { kind: "step_started"; call_id: CallId; step: string }
  | { kind: "step_progress"; call_id: CallId; step: string; fraction: number }
  | { kind: "step_finished"; call_id: CallId; step: string }
  | { kind: "finished"; call_id: CallId }
  | { kind: "failed"; call_id: CallId; step: string; error: string };

export const api = {
  listCalls: () => invoke<Call[]>("list_calls"),
  getCall: (id: CallId) => invoke<CallDetail>("get_call", { id }),
  /** Every call's summary, keyed by call; calls without one are missing. */
  summaries: () => invoke<Record<CallId, Summary>>("summaries"),
  importFiles: (paths: string[]) => invoke<Call[]>("import_files", { paths }),
  processCall: (id: CallId) => invoke<void>("process_call", { id }),
  renameCall: (id: CallId, title: string) => invoke<Call>("rename_call", { id, title }),
  /** `null` takes the call out of any folder; a new name creates the folder. */
  moveCall: (id: CallId, folder: string | null) => invoke<Call>("move_call", { id, folder }),
  /** To the macOS Trash. Not while recording or processing. */
  trashCall: (id: CallId) => invoke<void>("trash_call", { id }),
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
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<void>("update_settings", { settings }),
  /** On `page` if given. */
  openSettings: (page?: SettingsPage) => invoke<void>("open_settings", { page }),
  localModels: (kind: "whisper" | "vad") => invoke<LocalModel[]>("local_models", { kind }),
  deleteLocalModel: (name: string) => invoke<void>("delete_local_model", { name }),
  /** Fails if Ollama is not running. */
  ollamaModels: () => invoke<OllamaModel[]>("ollama_models"),
  deleteOllamaModel: (name: string) => invoke<void>("delete_ollama_model", { name }),
  downloads: () => invoke<(DownloadTarget & { fraction: number })[]>("downloads"),
  /** `token` only for the diarization model: a Hugging Face token, used once and not stored. */
  startDownload: (target: DownloadTarget, token?: string) => invoke<void>("start_download", { target, token }),
  cancelDownload: (target: DownloadTarget) => invoke<void>("cancel_download", { target }),
  /** The call's audio for the player; made on the spot for calls that do not have it yet. */
  playback: (id: CallId) => invoke<string>("playback", { id }),
  diarizationReady: () => invoke<boolean>("diarization_ready"),
  /** Renames a speaker throughout the call's transcript. */
  renameSpeaker: (id: CallId, from: string, to: string) => invoke<void>("rename_speaker", { id, from, to }),
  notionConnect: (token: string) => invoke<string>("notion_connect", { token }),
  notionDisconnect: () => invoke<void>("notion_disconnect"),
  notionPage: (link: string) => invoke<NotionPage>("notion_page", { link }),
  setFolderNotion: (name: string, page: NotionPage | null) => invoke<void>("set_folder_notion", { name, page }),
  notionStatus: (id: CallId) => invoke<NotionStatus>("notion_status", { id }),
  setCallNotion: (id: CallId, page: NotionPage | null) => invoke<NotionStatus>("set_call_notion", { id, page }),
  /** Creates or updates the call's page, named after it, under its parent page. */
  syncNotion: (id: CallId) => invoke<NotionStatus>("sync_notion", { id }),
  /** How many voices the user's voice was learnt from; null if it was not. */
  myVoice: () => invoke<number | null>("my_voice"),
  forgetMyVoice: () => invoke<void>("forget_my_voice"),
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

/** The pages of the settings window. */
export type SettingsPage = "general" | "transcription" | "summary" | "notion";

/** The speaker label of the person using the app. */
export const me = (settings: Settings) => settings.user_name.trim() || "Yo";

export const onSettingsChanged = (handler: () => void) => listen("settings_changed", () => handler());
export const onDownload = (handler: (event: DownloadEvent) => void) =>
  listen<DownloadEvent>("download", (e) => handler(e.payload));

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
  playback: "Preparando el audio",
  diarize: "Separando hablantes",
  name_speakers: "Identificando hablantes",
  summarize: "Resumiendo",
};

export const MEDIA_EXTENSIONS = ["mp4", "mkv", "mov", "webm", "m4a", "mp3", "wav", "aac", "flac", "ogg"];
