//! Model downloads, running in the background: whisper.cpp files and Ollama pulls. They
//! outlive the settings window, which picks them up again when it reopens.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use calls_engines::models::{self, ModelKind};
use calls_engines::ollama;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

/// Event with every change of a download: progress, then done, failed or cancelled.
const DOWNLOAD_EVENT: &str = "download";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A whisper.cpp transcription model.
    Whisper,
    /// A voice activity detection model.
    Vad,
    /// A model pulled into Ollama.
    Ollama,
}

/// What is being downloaded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Target {
    pub kind: Kind,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
enum Update {
    Progress { fraction: f32 },
    Done,
    Failed { error: String },
    Cancelled,
}

#[derive(Serialize, Clone)]
struct DownloadEvent {
    #[serde(flatten)]
    target: Target,
    #[serde(flatten)]
    update: Update,
}

/// A download in progress, for a window that opens after it started.
#[derive(Serialize)]
pub struct Running {
    #[serde(flatten)]
    target: Target,
    fraction: f32,
}

struct Active {
    fraction: f32,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Downloads {
    active: Mutex<HashMap<Target, Active>>,
}

impl Downloads {
    pub fn running(&self) -> Vec<Running> {
        self.lock().iter().map(|(target, a)| Running { target: target.clone(), fraction: a.fraction }).collect()
    }

    /// Starts downloading `target` on its own thread. Starting one already running does nothing.
    pub fn start(&self, app: &AppHandle, target: Target, ollama_url: String) {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = self.lock();
            if active.contains_key(&target) {
                return;
            }
            active.insert(target.clone(), Active { fraction: 0.0, cancel: cancel.clone() });
        }
        let app = app.clone();
        std::thread::spawn(move || {
            let progress = |fraction: f32| {
                if let Some(st) = app.try_state::<AppState>()
                    && let Some(active) = st.downloads.lock().get_mut(&target)
                {
                    active.fraction = fraction;
                }
                emit(&app, &target, Update::Progress { fraction });
            };
            let result = match target.kind {
                Kind::Whisper => models::download(ModelKind::Whisper, &target.name, &progress, &cancel),
                Kind::Vad => models::download(ModelKind::Vad, &target.name, &progress, &cancel),
                Kind::Ollama => ollama::pull(&ollama_url, &target.name, &progress, &cancel),
            };
            if let Some(st) = app.try_state::<AppState>() {
                st.downloads.lock().remove(&target);
            }
            let update = match result {
                Ok(()) => Update::Done,
                Err(_) if cancel.load(Ordering::Relaxed) => Update::Cancelled,
                Err(e) => Update::Failed { error: format!("{e:#}") },
            };
            emit(&app, &target, update);
        });
    }

    pub fn cancel(&self, target: &Target) {
        if let Some(active) = self.lock().get(target) {
            active.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Target, Active>> {
        self.active.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn emit(app: &AppHandle, target: &Target, update: Update) {
    let _ = app.emit(DOWNLOAD_EVENT, DownloadEvent { target: target.clone(), update });
}
