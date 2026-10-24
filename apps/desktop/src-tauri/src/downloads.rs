//! Model downloads, running in the background: whisper.cpp files and Ollama pulls. They
//! outlive the settings window, which picks them up again when it reopens.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use calls_engines::models::{self, ModelKind};
use calls_engines::{ollama, pyannote};
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
    /// The speaker diarization model (pyannote).
    Diarization,
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

/// The download itself: reports progress, and stops early when cancelled if it can.
pub type Job = Box<dyn FnOnce(&dyn Fn(f32), &AtomicBool) -> anyhow::Result<()> + Send>;

/// The job that downloads `target`. `ollama_url` is where Ollama listens; `token` unlocks
/// the diarization model on Hugging Face (used for that download only, never stored).
pub fn job(target: &Target, ollama_url: String, token: Option<String>) -> Job {
    let name = target.name.clone();
    match target.kind {
        Kind::Whisper => {
            Box::new(move |progress, cancel| models::download(ModelKind::Whisper, &name, progress, cancel))
        }
        Kind::Vad => Box::new(move |progress, cancel| models::download(ModelKind::Vad, &name, progress, cancel)),
        Kind::Ollama => Box::new(move |progress, cancel| ollama::pull(&ollama_url, &name, progress, cancel)),
        Kind::Diarization => Box::new(move |progress, _| match token {
            Some(token) => pyannote::download(&token, progress),
            None => anyhow::bail!("a Hugging Face token is needed to download the model"),
        }),
    }
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

    /// Runs `job` (the download of `target`) on its own thread, reporting its progress.
    /// Starting one already running does nothing.
    pub fn start(&self, app: &AppHandle, target: Target, job: Job) {
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
            let result = job(&progress, &cancel);
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
