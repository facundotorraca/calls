// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod downloads;
mod island;
mod recorder;

use std::sync::{Arc, RwLock};

use calls_core::queue::Queue;
use calls_core::{CallId, Event, Library, Settings, Status, settings};
use calls_engines::default_pipeline;
use downloads::Downloads;
use recorder::Recorder;
use tauri::menu::{Menu, MenuItemBuilder, MenuItemKind, PredefinedMenuItem};
use tauri::webview::Color;
use tauri::{AppHandle, Emitter, Manager, RunEvent, Theme, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Event the UI listens to for pipeline progress.
const PIPELINE_EVENT: &str = "pipeline";
/// Event telling the UI the list of calls changed (e.g. a recording started).
pub const LIBRARY_EVENT: &str = "library_changed";

/// Settings can change while the app runs (the settings window); everything reads them
/// from here when it needs them instead of keeping a copy.
pub type SharedSettings = Arc<RwLock<Settings>>;

pub struct AppState {
    pub settings: SharedSettings,
    pub library: Library,
    pub queue: Queue,
    pub recorder: Recorder,
    pub downloads: Downloads,
}

impl AppState {
    /// The settings as they are now.
    pub fn settings(&self) -> Settings {
        read(&self.settings)
    }
}

fn read(settings: &SharedSettings) -> Settings {
    settings.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let loaded = Settings::load_or_default(&settings::settings_path())?;
            let library = Library::open(&loaded.library_dir)?;
            let recorder = Recorder::spawn(app.handle(), &loaded);
            let settings = Arc::new(RwLock::new(loaded));
            let queue = spawn_queue(app.handle().clone(), library.clone(), settings.clone());
            resume_interrupted(&library, &queue)?;

            island::configure(app.handle());
            app.manage(AppState { settings, library, queue, recorder, downloads: Downloads::default() });

            app.set_menu(menu(app.handle())?)?;
            app.on_menu_event(|app, event| {
                if event.id() == "settings" {
                    open_settings(app);
                }
            });
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
            commands::rename_call,
            commands::move_call,
            commands::list_folders,
            commands::create_folder,
            commands::rename_folder,
            commands::delete_folder,
            commands::set_folder_color,
            commands::get_settings,
            commands::update_settings,
            commands::open_settings,
            commands::local_models,
            commands::delete_local_model,
            commands::ollama_models,
            commands::delete_ollama_model,
            commands::downloads,
            commands::start_download,
            commands::cancel_download,
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

/// The standard macOS menu, with "Ajustes…" (⌘,) where every Mac app has it.
fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::default(app)?;
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.first() {
        let settings = MenuItemBuilder::with_id("settings", "Ajustes…").accelerator("CmdOrCtrl+,").build(app)?;
        // After "About" and its separator.
        app_menu.insert_items(&[&settings, &PredefinedMenuItem::separator(app)?], 2)?;
    }
    Ok(menu)
}

/// Opens the settings window, or brings it to the front if it is already open.
pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("Ajustes")
        .inner_size(840.0, 620.0)
        .min_inner_size(700.0, 480.0)
        .theme(Some(Theme::Dark))
        .background_color(Color(7, 7, 7, 255))
        .build();
    if let Err(e) = built {
        eprintln!("could not open settings: {e:#}");
    }
}

/// Picks up work cut short by quitting the app: calls waiting for or in the middle of
/// processing, and recordings (the audio sidecar finalizes the files when the app goes away).
fn resume_interrupted(library: &Library, queue: &Queue) -> anyhow::Result<()> {
    for call in library.list()? {
        match call.status {
            Status::Pending | Status::Processing { .. } => queue.push(call.id),
            Status::Recording => {
                let dir = library.dir(&call.id);
                library.update(&call.id, |call| {
                    call.tracks.retain(|t| dir.join(&t.file).exists());
                    call.status = Status::Pending;
                })?;
                queue.push(call.id);
            }
            Status::Ready | Status::Failed { .. } => {}
        }
    }
    Ok(())
}

fn spawn_queue(app: AppHandle, library: Library, settings: SharedSettings) -> Queue {
    Queue::spawn(move |id| {
        let emit = |event: Event| {
            if let Some(st) = app.try_state::<AppState>() {
                st.recorder.on_pipeline_event(&app, &event);
            }
            let _ = app.emit(PIPELINE_EVENT, event);
        };
        // Read for every call, so a model chosen in the settings applies to the next one.
        let result = default_pipeline(&read(&settings)).and_then(|pipeline| pipeline.run(&library, id, &emit));
        if let Err(e) = result {
            eprintln!("{id}: {e:#}");
            mark_failed_if_unhandled(&library, id, &e, &emit);
        }
    })
}

/// Errors raised before any step ran (e.g. whisper-cli missing) are not recorded by the
/// pipeline itself; record them so the call does not stay pending forever.
fn mark_failed_if_unhandled(library: &Library, id: &CallId, error: &anyhow::Error, emit: &dyn Fn(Event)) {
    let Ok(call) = library.load(id) else { return };
    if matches!(call.status, Status::Failed { .. }) {
        return;
    }
    let error = format!("{error:#}");
    let failed = Status::Failed { step: "setup".into(), error: error.clone() };
    if library.update(id, |call| call.status = failed).is_ok() {
        emit(Event::Failed { call_id: id.clone(), step: "setup".into(), error });
    }
}
