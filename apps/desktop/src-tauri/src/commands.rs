//! Commands the UI can invoke. Each one is a thin wrapper over the core; keep logic out of here.
//! Their TypeScript counterparts live in `src/api.ts`.

use std::path::PathBuf;

use calls_core::{Call, CallId, Status, Transcript};
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
}

#[tauri::command]
pub async fn list_calls(state: State<'_, AppState>) -> CmdResult<Vec<Call>> {
    state.library.list().map_err(to_string)
}

#[tauri::command]
pub async fn get_call(state: State<'_, AppState>, id: CallId) -> CmdResult<CallDetail> {
    let call = state.library.load(&id).map_err(to_string)?;
    let transcript = state.library.transcript(&id).map_err(to_string)?;
    Ok(CallDetail { dir: state.library.dir(&id), call, transcript })
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

/// Retries a failed call. Steps that already succeeded are not run again.
#[tauri::command]
pub async fn process_call(state: State<'_, AppState>, id: CallId) -> CmdResult<()> {
    let mut call = state.library.load(&id).map_err(to_string)?;
    call.status = Status::Pending;
    state.library.save(&call).map_err(to_string)?;
    state.queue.push(id);
    Ok(())
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
