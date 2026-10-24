//! Commands the UI can invoke. Each one is a thin wrapper over the core; keep logic out of here.
//! Their TypeScript counterparts live in `src/api.ts`.

use std::path::PathBuf;

use calls_core::summary::Summary;
use calls_core::{Call, CallId, Folder, FolderColor, Status, Transcript};
use calls_engines::Doctor;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::recorder::Island;
use crate::{AppState, show_main};

type CmdResult<T> = Result<T, String>;

fn to_string(e: anyhow::Error) -> String {
    format!("{e:#}")
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

#[tauri::command]
pub async fn doctor(state: State<'_, AppState>) -> CmdResult<Doctor> {
    Ok(Doctor::check(&state.settings))
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
