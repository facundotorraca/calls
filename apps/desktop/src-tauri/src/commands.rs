//! Commands the UI can invoke. Each one is a thin wrapper over the core; keep logic out of here.
//! Their TypeScript counterparts live in `src/api.ts`.

use std::path::PathBuf;

use anyhow::bail;
use calls_core::steps::{self, PLAYBACK_FILE};
use calls_core::summary::Summary;
use calls_core::voices::MyVoice;
use calls_core::{Call, CallId, Folder, FolderColor, Settings, Status, Transcript, settings};
use calls_engines::models::{self, LocalModel, ModelKind};
use calls_engines::ollama::{self, InstalledModel};
use calls_engines::{Doctor, Ffmpeg, pyannote};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::downloads::{self, Running, Target};
use crate::recorder::Island;
use crate::{AppState, show_main};

/// Event telling every window the settings changed.
const SETTINGS_EVENT: &str = "settings_changed";

type CmdResult<T> = Result<T, String>;

fn to_string(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Runs blocking work (HTTP to Ollama, the filesystem) off the async runtime.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())?.map_err(to_string)
}

#[derive(Serialize)]
pub struct CallDetail {
    call: Call,
    dir: PathBuf,
    transcript: Option<Transcript>,
    summary: Option<Summary>,
}

#[tauri::command]
pub async fn list_calls(state: State<'_, AppState>) -> CmdResult<Vec<Call>> {
    state.library.list().map_err(to_string)
}

#[tauri::command]
pub async fn get_call(state: State<'_, AppState>, id: CallId) -> CmdResult<CallDetail> {
    let call = state.library.load(&id).map_err(to_string)?;
    let dir = state.library.dir(&id);
    let transcript = state.library.transcript(&id).map_err(to_string)?;
    let summary = Summary::load(&dir).map_err(to_string)?;
    Ok(CallDetail { dir, call, transcript, summary })
}

/// Registers the files as calls and queues them for processing.
#[tauri::command]
pub async fn import_files(state: State<'_, AppState>, paths: Vec<PathBuf>) -> CmdResult<Vec<Call>> {
    let mut calls = Vec::new();
    for path in paths {
        let call = state.library.import(&path).map_err(to_string)?;
        state.queue.push(call.id.clone());
        calls.push(call);
    }
    Ok(calls)
}

/// Runs whatever is missing for a call (a failed step, a summary): completed steps are skipped.
#[tauri::command]
pub async fn process_call(state: State<'_, AppState>, id: CallId) -> CmdResult<()> {
    state.library.update(&id, |call| call.status = Status::Pending).map_err(to_string)?;
    state.queue.push(id);
    Ok(())
}

#[tauri::command]
pub async fn rename_call(state: State<'_, AppState>, id: CallId, title: String) -> CmdResult<Call> {
    state.library.rename(&id, &title).map_err(to_string)
}

/// Moves a call into `folder` (created if missing), or out of any folder with `null`.
#[tauri::command]
pub async fn move_call(state: State<'_, AppState>, id: CallId, folder: Option<String>) -> CmdResult<Call> {
    state.library.move_to(&id, folder.as_deref()).map_err(to_string)
}

/// Moves a call to the Trash.
#[tauri::command]
pub async fn trash_call(state: State<'_, AppState>, id: CallId) -> CmdResult<()> {
    state.library.remove(&id, calls_engines::trash::move_to_trash).map_err(to_string)
}

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> CmdResult<Vec<Folder>> {
    state.library.folders().map_err(to_string)
}

/// Sets a folder's color, or clears it with `null`.
#[tauri::command]
pub async fn set_folder_color(state: State<'_, AppState>, name: String, color: Option<FolderColor>) -> CmdResult<()> {
    state.library.set_folder_color(&name, color).map_err(to_string)
}

#[tauri::command]
pub async fn create_folder(state: State<'_, AppState>, name: String) -> CmdResult<String> {
    state.library.create_folder(&name).map_err(to_string)
}

#[tauri::command]
pub async fn rename_folder(state: State<'_, AppState>, from: String, to: String) -> CmdResult<()> {
    state.library.rename_folder(&from, &to).map_err(to_string)
}

/// Deletes the folder only; its calls stay in the library.
#[tauri::command]
pub async fn delete_folder(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    state.library.delete_folder(&name).map_err(to_string)
}

/// The file to play for a call. Calls processed before the player existed get theirs made
/// now, which takes a few seconds.
#[tauri::command]
pub async fn playback(state: State<'_, AppState>, id: CallId) -> CmdResult<PathBuf> {
    let library = state.library.clone();
    blocking(move || {
        let dir = library.dir(&id);
        let path = dir.join(PLAYBACK_FILE);
        if path.is_file() {
            return Ok(path);
        }
        let call = library.load(&id)?;
        steps::make_playback(&call, &dir, &Ffmpeg::locate()?, &|_| {})
    })
    .await
}

#[tauri::command]
pub async fn doctor(state: State<'_, AppState>) -> CmdResult<Doctor> {
    let settings = state.settings();
    blocking(move || Ok(Doctor::check(&settings))).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings())
}

/// Saves the settings and applies them: the next call processed uses them.
#[tauri::command]
pub async fn update_settings(app: AppHandle, state: State<'_, AppState>, mut settings: Settings) -> CmdResult<()> {
    // The library is opened at launch; moving it is not something to do from under it.
    settings.library_dir = state.settings().library_dir;
    settings.save(&settings::settings_path()).map_err(to_string)?;
    *state.settings.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = settings;
    app.emit(SETTINGS_EVENT, ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_settings(app: AppHandle) -> CmdResult<()> {
    crate::open_settings(&app);
    Ok(())
}

/// The whisper.cpp models offered and the ones on disk.
#[tauri::command]
pub async fn local_models(kind: ModelKind) -> CmdResult<Vec<LocalModel>> {
    blocking(move || Ok(models::list(kind))).await
}

#[tauri::command]
pub async fn delete_local_model(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    let settings = state.settings();
    blocking(move || {
        if settings.whisper_model == name || settings.vad_model.as_deref() == Some(name.as_str()) {
            bail!("{name} is in use; choose another model first");
        }
        models::delete(&name)
    })
    .await
}

/// The models Ollama has. Fails if Ollama is not running.
#[tauri::command]
pub async fn ollama_models(state: State<'_, AppState>) -> CmdResult<Vec<InstalledModel>> {
    let url = state.settings().llm.base_url;
    blocking(move || ollama::installed(&url)).await
}

#[tauri::command]
pub async fn delete_ollama_model(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    let llm = state.settings().llm;
    blocking(move || {
        if llm.model == name {
            bail!("{name} is in use; choose another model first");
        }
        ollama::delete(&llm.base_url, &name)
    })
    .await
}

#[tauri::command]
pub async fn downloads(state: State<'_, AppState>) -> CmdResult<Vec<Running>> {
    Ok(state.downloads.running())
}

/// Starts a download in the background; progress arrives as `download` events. `token` is
/// only for the diarization model.
#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    target: Target,
    token: Option<String>,
) -> CmdResult<()> {
    let job = downloads::job(&target, state.settings().llm.base_url, token);
    state.downloads.start(&app, target, job);
    Ok(())
}

/// Whether the diarization model is downloaded.
#[tauri::command]
pub async fn diarization_ready() -> CmdResult<bool> {
    Ok(pyannote::is_downloaded())
}

/// Renames a speaker of a call. Naming one as the person using the app ("soy yo") also
/// teaches the app their voice, to recognize them in the next calls.
#[tauri::command]
pub async fn rename_speaker(state: State<'_, AppState>, id: CallId, from: String, to: String) -> CmdResult<()> {
    let voice = state.library.voice(&id, &from);
    // You can be split in two voices: each "soy yo" adds one more to yours.
    let is_me = to.trim() == state.settings().me();
    state.library.rename_speaker(&id, &from, &to, is_me).map_err(to_string)?;
    if is_me && let Some(voice) = voice {
        MyVoice::learn(&settings::my_voice_path(), &voice).map_err(to_string)?;
    }
    Ok(())
}

/// How many voices the app learnt the user's from; none if it has not.
#[tauri::command]
pub async fn my_voice() -> CmdResult<Option<u32>> {
    Ok(MyVoice::load(&settings::my_voice_path()).map(|v| v.samples))
}

#[tauri::command]
pub async fn forget_my_voice() -> CmdResult<()> {
    match std::fs::remove_file(settings::my_voice_path()) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, AppState>, target: Target) -> CmdResult<()> {
    state.downloads.cancel(&target);
    Ok(())
}

#[tauri::command]
pub async fn island_state(state: State<'_, AppState>) -> CmdResult<Island> {
    Ok(state.recorder.island())
}

/// Records the detected call, or everything the Mac plays if no call was detected.
#[tauri::command]
pub async fn start_recording(app: AppHandle, state: State<'_, AppState>) -> CmdResult<CallId> {
    state.recorder.start(&app, &state).map_err(to_string)
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    state.recorder.stop(&app);
    Ok(())
}

#[tauri::command]
pub async fn dismiss_island(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    state.recorder.dismiss(&app);
    Ok(())
}

/// Brings the main window to the front with the given call selected.
#[tauri::command]
pub async fn show_call(app: AppHandle, id: CallId) -> CmdResult<()> {
    show_main(&app);
    app.emit_to("main", "select_call", id).map_err(|e| e.to_string())
}
