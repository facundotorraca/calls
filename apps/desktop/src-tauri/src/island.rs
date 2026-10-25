//! The island: a small floating window at the top center of the screen, like the Dynamic
//! Island. It must show over full screen apps (a full screen Zoom call), never steal focus
//! and stay out of screen shares. It can be dragged elsewhere; it comes back there.

use std::fs;
use std::path::PathBuf;

use calls_core::settings;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Monitor, WebviewWindow};

const LABEL: &str = "island";
/// Gap below the menu bar.
const MARGIN_TOP: f64 = 6.0;
/// Where the user dragged it to, if they did.
const POSITION_FILE: &str = "island.json";

/// The island's place, in logical points: its horizontal center and its top. By the
/// center, so it stays put as it grows and shrinks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Place {
    center_x: f64,
    y: f64,
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// One time setup, from the main thread.
pub fn configure(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Some(window) = window(app)
        && let Ok(ptr) = window.ns_window()
    {
        use objc2_app_kit::{NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior as B, NSWindowSharingType};
        // SAFETY: Tauri hands us the live NSWindow of this webview window, and setup runs on the main thread.
        let ns_window = unsafe { &*ptr.cast::<NSWindow>() };
        ns_window.setLevel(NSStatusWindowLevel);
        ns_window.setCollectionBehavior(B::CanJoinAllSpaces | B::FullScreenAuxiliary | B::Stationary | B::IgnoresCycle);
        // Left out of screenshots and screen shares: it is for the person recording only.
        ns_window.setSharingType(NSWindowSharingType::None);
    }
}

pub fn show(app: &AppHandle) {
    let Some(window) = window(app) else { return };
    let place = saved().filter(|p| on_a_monitor(&window, *p)).or_else(|| default_place(&window));
    if let Some(place) = place {
        move_to(&window, place);
    }

    #[cfg(target_os = "macos")]
    {
        // `show()` would activate the app and steal focus from the call; order front without it.
        let target = window.clone();
        let _ = window.run_on_main_thread(move || {
            if let Ok(ptr) = target.ns_window() {
                // SAFETY: as in `configure`, and this closure runs on the main thread.
                let ns_window = unsafe { &*ptr.cast::<objc2_app_kit::NSWindow>() };
                ns_window.orderFrontRegardless();
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window.show();
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = window(app) {
        remember(&window);
        let _ = window.hide();
    }
}

/// Fits the window to what the island shows, keeping it centered where it is.
pub fn resize(app: &AppHandle, width: f64, height: f64) {
    let Some(window) = window(app) else { return };
    let place = current(&window);
    let _ = window.set_size(LogicalSize::new(width, height));
    if let Some(place) = place {
        move_to(&window, place);
    }
}

/// Back to the top center, forgetting where it was dragged to.
pub fn reset_place(app: &AppHandle) {
    let _ = fs::remove_file(position_path());
    if let Some(window) = window(app)
        && let Some(place) = default_place(&window)
    {
        move_to(&window, place);
    }
}

/// Saves where the island is, if the user dragged it away from the default place.
fn remember(window: &WebviewWindow) {
    let (Some(place), Some(default)) = (current(window), default_place(window)) else { return };
    let moved = (place.center_x - default.center_x).abs() > 2.0 || (place.y - default.y).abs() > 2.0;
    if moved {
        if let Ok(json) = serde_json::to_vec(&place) {
            let _ = fs::write(position_path(), json);
        }
    } else {
        let _ = fs::remove_file(position_path());
    }
}

fn saved() -> Option<Place> {
    serde_json::from_slice(&fs::read(position_path()).ok()?).ok()
}

fn position_path() -> PathBuf {
    settings::data_dir().join(POSITION_FILE)
}

fn scale(window: &WebviewWindow) -> f64 {
    window.scale_factor().unwrap_or(1.0)
}

fn current(window: &WebviewWindow) -> Option<Place> {
    let scale = scale(window);
    let position = window.outer_position().ok()?.to_logical::<f64>(scale);
    let size = window.outer_size().ok()?.to_logical::<f64>(scale);
    Some(Place { center_x: position.x + size.width / 2.0, y: position.y })
}

fn move_to(window: &WebviewWindow, place: Place) {
    let width = window.outer_size().map(|s| s.to_logical::<f64>(scale(window)).width).unwrap_or(0.0);
    let _ = window.set_position(LogicalPosition::new(place.center_x - width / 2.0, place.y));
}

/// Top center of the main screen, below the menu bar.
fn default_place(window: &WebviewWindow) -> Option<Place> {
    let monitor = window.primary_monitor().ok()??;
    let (x, y, width, _) = work_area(&monitor);
    Some(Place { center_x: x + width / 2.0, y: y + MARGIN_TOP })
}

/// Whether the place is on a screen still connected, so the island is never lost off screen.
fn on_a_monitor(window: &WebviewWindow, place: Place) -> bool {
    window.available_monitors().unwrap_or_default().iter().any(|monitor| {
        let (x, y, width, height) = work_area(monitor);
        (x..x + width).contains(&place.center_x) && (y..y + height).contains(&place.y)
    })
}

/// A monitor's work area (without menu bar and Dock) in logical points: x, y, width, height.
fn work_area(monitor: &Monitor) -> (f64, f64, f64, f64) {
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let position = area.position.to_logical::<f64>(scale);
    let size = area.size.to_logical::<f64>(scale);
    (position.x, position.y, size.width, size.height)
}
