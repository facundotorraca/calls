// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod island;
mod recorder;

use calls_core::queue::Queue;
use calls_core::{CallId, Event, Library, Settings, Status, settings};
use calls_engines::default_pipeline;
use recorder::Recorder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

/// Event the UI listens to for pipeline progress.
const PIPELINE_EVENT: &str = "pipeline";
/// Event telling the UI the list of calls changed (e.g. a recording started).
pub const LIBRARY_EVENT: &str = "library_changed";

pub struct AppState {
    pub settings: Settings,
    pub library: Library,
    pub queue: Queue,
    pub recorder: Recorder,
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings = Settings::load_or_default(&settings::settings_path())?;
            let library = Library::open(&settings.library_dir)?;
            let queue = spawn_queue(app.handle().clone(), library.clone(), settings.clone());
            resume_interrupted(&library, &queue)?;

            island::configure(app.handle());
            let recorder = Recorder::spawn(app.handle(), &settings);
            app.manage(AppState { settings, library, queue, recorder });
            Ok(())
        })
        // Closing the main window keeps the app running in the background, watching for calls.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_calls,
            commands::get_call,
            commands::import_files,
            commands::process_call,
            commands::doctor,
            commands::island_state,
            commands::start_recording,
            commands::stop_recording,
            commands::dismiss_island,
            commands::show_call,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the app");

    app.run(|app, event| {
        // Clicking the dock icon brings the main window back.
        if let RunEvent::Reopen { .. } = event {
            show_main(app);
        }
    });
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Picks up work cut short by quitting the app: calls waiting for or in the middle of
/// processing, and recordings (the audio sidecar finalizes the files when the app goes away).
fn resume_interrupted(library: &Library, queue: &Queue) -> anyhow::Result<()> {
    for mut call in library.list()? {
        match call.status {
            Status::Pending | Status::Processing { .. } => queue.push(call.id),
            Status::Recording => {
                let dir = library.dir(&call.id);
                call.tracks.retain(|t| dir.join(&t.file).exists());
                call.status = Status::Pending;
                library.save(&call)?;
                queue.push(call.id);
            }
            Status::Ready | Status::Failed { .. } => {}
        }
    }
    Ok(())
}

fn spawn_queue(app: AppHandle, library: Library, settings: Settings) -> Queue {
    Queue::spawn(move |id| {
        let emit = |event: Event| {
            if let Some(st) = app.try_state::<AppState>() {
                st.recorder.on_pipeline_event(&app, &event);
            }
            let _ = app.emit(PIPELINE_EVENT, event);
        };
        let result = default_pipeline(&settings).and_then(|pipeline| pipeline.run(&library, id, &emit));
        if let Err(e) = result {
            eprintln!("{id}: {e:#}");
            mark_failed_if_unhandled(&library, id, &e, &emit);
        }
    })
}

/// Errors raised before any step ran (e.g. whisper-cli missing) are not recorded by the
/// pipeline itself; record them so the call does not stay pending forever.
fn mark_failed_if_unhandled(library: &Library, id: &CallId, error: &anyhow::Error, emit: &dyn Fn(Event)) {
    let Ok(mut call) = library.load(id) else { return };
    if matches!(call.status, Status::Failed { .. }) {
        return;
    }
    let error = format!("{error:#}");
    call.status = Status::Failed { step: "setup".into(), error: error.clone() };
    if library.save(&call).is_ok() {
        emit(Event::Failed { call_id: id.clone(), step: "setup".into(), error });
    }
}
