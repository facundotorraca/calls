//! Recording flow: call detected -> island asks -> recording -> processing -> done.
//!
//! The audio sidecar reports what apps are doing with audio; [`CallDetector`] turns that into
//! call start/end, and this module drives the island and the library accordingly.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use calls_core::detect::{AudioProcess, CallApp, CallDetector, Detection};
use calls_core::{CallId, Event, Settings, Status};
use calls_engines::audio::{AudioDaemon, AudioEvent};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{AppState, LIBRARY_EVENT, island};

const ISLAND_EVENT: &str = "island";
const LEVELS_EVENT: &str = "audio_levels";
/// How long "transcript ready" stays on screen.
const DONE_VISIBLE_FOR: Duration = Duration::from_secs(8);

/// What the island shows. The island window renders exactly this.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Island {
    Hidden,
    Detected { app_name: String },
    Recording { call_id: CallId, app_name: Option<String>, started_at_ms: u64 },
    Processing {
        call_id: CallId,
        step: String,
        fraction: f32,
        /// Some step was left for later (the summary, when there was no memory for it).
        postponed: bool,
    },
    Done { call_id: CallId, title: String, failed: bool, postponed: bool },
}

struct Recording {
    call_id: CallId,
    /// The detected call being recorded, `None` if started by hand.
    app: Option<CallApp>,
}

struct State {
    detector: CallDetector,
    /// Call in progress according to the detector, recorded or not.
    call: Option<CallApp>,
    recording: Option<Recording>,
    island: Island,
}

pub struct Recorder {
    /// `None` if the sidecar is missing: importing files still works.
    daemon: Option<AudioDaemon>,
    state: Mutex<State>,
}

#[derive(Clone, Serialize)]
struct Levels {
    mic: f32,
    system: f32,
}

impl Recorder {
    pub fn spawn(app: &AppHandle, settings: &Settings) -> Recorder {
        let handle = app.clone();
        let daemon = AudioDaemon::spawn(move |event| on_audio_event(&handle, event))
            .inspect_err(|e| eprintln!("recording disabled: {e:#}"))
            .ok();
        let state = State {
            detector: CallDetector::new(settings.call_apps.clone()),
            call: None,
            recording: None,
            island: Island::Hidden,
        };
        Recorder { daemon, state: Mutex::new(state) }
    }

    pub fn island(&self) -> Island {
        self.lock().island.clone()
    }

    pub fn start(&self, app: &AppHandle, st: &AppState) -> Result<CallId> {
        let mut s = self.lock();
        self.start_locked(app, st, &mut s)
    }

    pub fn stop(&self, app: &AppHandle) {
        let mut s = self.lock();
        self.stop_locked(app, &mut s);
    }

    /// Closes the island when it is only informing (detected call, transcript ready).
    pub fn dismiss(&self, app: &AppHandle) {
        let mut s = self.lock();
        if matches!(s.island, Island::Detected { .. } | Island::Done { .. }) {
            set_island(app, &mut s, Island::Hidden);
        }
    }

    /// Follows the processing of the call just recorded, to show its progress on the island.
    pub fn on_pipeline_event(&self, app: &AppHandle, event: &Event) {
        let mut s = self.lock();
        let Island::Processing { call_id, postponed, .. } = &s.island else { return };
        let (call_id, postponed) = (call_id.clone(), *postponed);
        let title = |id: &CallId| {
            app.try_state::<AppState>().and_then(|st| st.library.load(id).ok()).map(|c| c.title).unwrap_or_default()
        };
        let next = match event {
            Event::StepStarted { call_id: id, step } if *id == call_id => {
                Island::Processing { call_id, step: step.clone(), fraction: 0.0, postponed }
            }
            Event::StepProgress { call_id: id, step, fraction } if *id == call_id => {
                Island::Processing { call_id, step: step.clone(), fraction: *fraction, postponed }
            }
            Event::StepPostponed { call_id: id, step } if *id == call_id => {
                Island::Processing { call_id, step: step.clone(), fraction: 1.0, postponed: true }
            }
            Event::Finished { call_id: id } if *id == call_id => {
                Island::Done { title: title(&call_id), call_id, failed: false, postponed }
            }
            Event::Failed { call_id: id, .. } if *id == call_id => {
                Island::Done { title: title(&call_id), call_id, failed: true, postponed }
            }
            _ => return,
        };
        if let Island::Done { call_id, .. } = &next {
            hide_later(app.clone(), call_id.clone());
        }
        set_island(app, &mut s, next);
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("poisoned")
    }

    fn on_audio_apps(&self, app: &AppHandle, st: &AppState, apps: &[AudioProcess]) {
        let settings = st.settings();
        if !settings.detect_calls {
            return;
        }
        let mut s = self.lock();
        match s.detector.update(apps, Instant::now()) {
            Some(Detection::Started(call)) => {
                s.call = Some(call.clone());
                if s.recording.is_some() {
                    return;
                }
                if settings.auto_record {
                    if let Err(e) = self.start_locked(app, st, &mut s) {
                        eprintln!("could not start recording: {e:#}");
                    }
                } else {
                    set_island(app, &mut s, Island::Detected { app_name: call.name });
                }
            }
            Some(Detection::Ended(call)) => {
                s.call = None;
                if s.recording.as_ref().is_some_and(|r| r.app.as_ref() == Some(&call)) {
                    self.stop_locked(app, &mut s);
                } else if matches!(s.island, Island::Detected { .. }) {
                    set_island(app, &mut s, Island::Hidden);
                }
            }
            None => {}
        }
    }

    fn start_locked(&self, app: &AppHandle, st: &AppState, s: &mut State) -> Result<CallId> {
        if let Some(recording) = &s.recording {
            return Ok(recording.call_id.clone());
        }
        let daemon = self.daemon.as_ref().context("recording is not available: calls-audio not found")?;

        let call_app = s.call.clone();
        let title = call_app.as_ref().map(|a| format!("Llamada en {}", a.name)).unwrap_or_else(|| "Grabación".into());
        let mut call = st.library.create_recording(&title, &st.settings().me())?;
        if let Err(e) = daemon.start(&st.library.dir(&call.id), call_app.as_ref().map(|a| a.bundle_prefix.as_str())) {
            call.status = Status::Failed { step: "recording".into(), error: format!("{e:#}") };
            st.library.save(&call)?;
            return Err(e);
        }

        s.recording = Some(Recording { call_id: call.id.clone(), app: call_app.clone() });
        let island = Island::Recording {
            call_id: call.id.clone(),
            app_name: call_app.map(|a| a.name),
            started_at_ms: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
        };
        set_island(app, s, island);
        let _ = app.emit(LIBRARY_EVENT, ());
        Ok(call.id)
    }

    /// Asks the sidecar to stop; the call is queued once it confirms the files are complete.
    fn stop_locked(&self, app: &AppHandle, s: &mut State) {
        let Some(recording) = &s.recording else { return };
        let call_id = recording.call_id.clone();
        if let Some(daemon) = &self.daemon
            && let Err(e) = daemon.stop()
        {
            eprintln!("could not stop recording: {e:#}");
        }
        set_island(app, s, Island::Processing { call_id, step: "saving".into(), fraction: 0.0, postponed: false });
    }

    fn on_stopped(&self, app: &AppHandle, st: &AppState, files: Vec<String>, offsets_ms: HashMap<String, u64>) {
        let Some(recording) = self.lock().recording.take() else { return };
        let queued = st.library.update(&recording.call_id, |call| {
            call.tracks.retain(|t| files.contains(&t.file));
            for track in &mut call.tracks {
                track.offset_ms = offsets_ms.get(&track.file).copied().unwrap_or(0);
            }
            call.status = Status::Pending;
        });
        match queued {
            Ok(_) => st.queue.push(recording.call_id),
            Err(e) => eprintln!("{}: {e:#}", recording.call_id),
        }
        let _ = app.emit(LIBRARY_EVENT, ());
    }
}

fn on_audio_event(app: &AppHandle, event: AudioEvent) {
    let Some(st) = app.try_state::<AppState>() else { return };
    match event {
        AudioEvent::AudioApps { apps } => st.recorder.on_audio_apps(app, &st, &apps),
        AudioEvent::Levels { mic, system } => {
            let _ = app.emit(LEVELS_EVENT, Levels { mic, system });
        }
        AudioEvent::RecordingStopped { files, offsets_ms } => st.recorder.on_stopped(app, &st, files, offsets_ms),
        AudioEvent::Error { message } => eprintln!("calls-audio: {message}"),
        AudioEvent::Ready | AudioEvent::RecordingStarted => {}
    }
}

fn set_island(app: &AppHandle, s: &mut State, island: Island) {
    let was_hidden = s.island == Island::Hidden;
    s.island = island;
    let _ = app.emit(ISLAND_EVENT, &s.island);
    match (&s.island, was_hidden) {
        (Island::Hidden, _) => island::hide(app),
        (_, true) => island::show(app),
        _ => {}
    }
}

fn hide_later(app: AppHandle, call_id: CallId) {
    thread::spawn(move || {
        thread::sleep(DONE_VISIBLE_FOR);
        let Some(st) = app.try_state::<AppState>() else { return };
        let mut s = st.recorder.lock();
        if matches!(&s.island, Island::Done { call_id: id, .. } if *id == call_id) {
            set_island(&app, &mut s, Island::Hidden);
        }
    });
}
