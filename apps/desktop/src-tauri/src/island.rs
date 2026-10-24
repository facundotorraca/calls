//! The island: a small floating window at the top center of the screen, like the Dynamic
//! Island. It must show over full screen apps (a full screen Zoom call) and never steal focus.

use tauri::{AppHandle, LogicalPosition, Manager, WebviewWindow};

const LABEL: &str = "island";
/// Gap below the menu bar.
const MARGIN_TOP: f64 = 6.0;

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// One time setup, from the main thread.
pub fn configure(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Some(window) = window(app)
        && let Ok(ptr) = window.ns_window()
    {
        use objc2_app_kit::{NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior as B};
        // SAFETY: Tauri hands us the live NSWindow of this webview window, and setup runs on the main thread.
        let ns_window = unsafe { &*ptr.cast::<NSWindow>() };
        ns_window.setLevel(NSStatusWindowLevel);
        ns_window.setCollectionBehavior(B::CanJoinAllSpaces | B::FullScreenAuxiliary | B::Stationary | B::IgnoresCycle);
    }
}

pub fn show(app: &AppHandle) {
    let Some(window) = window(app) else { return };
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let (x, y) = (area.position.x as f64 / scale, area.position.y as f64 / scale);
        let area_width = area.size.width as f64 / scale;
        let width = window.outer_size().map(|s| s.width as f64 / scale).unwrap_or(0.0);
        let _ = window.set_position(LogicalPosition::new(x + (area_width - width) / 2.0, y + MARGIN_TOP));
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
        let _ = window.hide();
    }
}
