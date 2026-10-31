//! Updates from apps.ftor.dev, where CI publishes (.github/workflows/release.yml).
//! Checks on launch and every few hours and downloads in the background; then the menu bar
//! offers to restart into the new version. Never on its own: it would cut a recording short.

use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::tray;

const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// A downloaded update, waiting for the user to restart.
#[derive(Default)]
struct Ready(Mutex<Option<(Update, Vec<u8>)>>);

pub fn spawn(app: &AppHandle) {
    // Dev builds (`make dev`) would replace the installed app.
    if cfg!(debug_assertions) {
        return;
    }
    app.manage(Ready::default());
    let app = app.clone();
    thread::spawn(move || {
        loop {
            match tauri::async_runtime::block_on(download(&app)) {
                Ok(Some((update, bytes))) => {
                    let version = update.version.clone();
                    *lock(&app.state::<Ready>()) = Some((update, bytes));
                    tray::offer_update(&app, &version);
                    return;
                }
                Ok(None) => {}
                Err(e) => eprintln!("update check failed: {e:#}"),
            }
            thread::sleep(EVERY);
        }
    });
}

async fn download(app: &AppHandle) -> tauri_plugin_updater::Result<Option<(Update, Vec<u8>)>> {
    let Some(update) = app.updater()?.check().await? else { return Ok(None) };
    let bytes = update.download(|_, _| {}, || {}).await?;
    Ok(Some((update, bytes)))
}

/// Replaces the app with the downloaded version and opens it.
pub fn install_and_restart(app: &AppHandle) {
    let Some(ready) = app.try_state::<Ready>() else { return };
    let Some((update, bytes)) = lock(&ready).take() else { return };
    match update.install(bytes) {
        Ok(()) => app.restart(),
        Err(e) => eprintln!("could not install the update: {e:#}"),
    }
}

fn lock(ready: &Ready) -> std::sync::MutexGuard<'_, Option<(Update, Vec<u8>)>> {
    ready.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
