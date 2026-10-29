//! The menu bar icon: always there, even with the window closed. The full stop of the "c."
//! shows while a call is being recorded, and its menu starts or stops the recording.

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

use crate::{AppState, show_main};

const IDLE: &[u8] = include_bytes!("../icons/tray.png");
const RECORDING: &[u8] = include_bytes!("../icons/tray-recording.png");

const RECORD: &str = "tray_record";
const OPEN: &str = "tray_open";

struct Tray {
    icon: TrayIcon,
    record: MenuItem<Wry>,
    recording: bool,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let record = MenuItem::with_id(app, RECORD, "Grabar", true, None::<&str>)?;
    let open = MenuItem::with_id(app, OPEN, "Abrir Calls", true, None::<&str>)?;
    // Same id as the app menu's, which opens it.
    let settings = MenuItem::with_id(app, "settings", "Ajustes…", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = PredefinedMenuItem::quit(app, Some("Salir de Calls"))?;
    let menu = Menu::with_items(app, &[&record, &open, &settings, &separator, &quit])?;

    let icon = TrayIconBuilder::with_id("calls")
        .icon(Image::from_bytes(IDLE)?)
        .icon_as_template(true)
        .tooltip("Calls")
        .menu(&menu)
        .on_menu_event(on_menu_event)
        .build(app)?;
    app.manage(Mutex::new(Tray { icon, record, recording: false }));
    Ok(())
}

/// Shows whether a call is being recorded.
pub fn set_recording(app: &AppHandle, recording: bool) {
    let Some(tray) = app.try_state::<Mutex<Tray>>() else { return };
    let mut tray = tray.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if tray.recording == recording {
        return;
    }
    tray.recording = recording;
    let (icon, label, tooltip) =
        if recording { (RECORDING, "Detener grabación", "Calls · grabando") } else { (IDLE, "Grabar", "Calls") };
    let _ = tray.icon.set_icon(Image::from_bytes(icon).ok());
    let _ = tray.icon.set_icon_as_template(true);
    let _ = tray.icon.set_tooltip(Some(tooltip));
    let _ = tray.record.set_text(label);
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        OPEN => show_main(app),
        RECORD => {
            // Off the main thread: the recorder updates this icon while holding its lock, and
            // that waits for the main thread.
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let Some(st) = app.try_state::<AppState>() else { return };
                let recording = app
                    .try_state::<Mutex<Tray>>()
                    .is_some_and(|t| t.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).recording);
                if recording {
                    st.recorder.stop(&app);
                } else if let Err(e) = st.recorder.start(&app, &st) {
                    eprintln!("could not start recording: {e:#}");
                }
            });
        }
        _ => {}
    }
}
