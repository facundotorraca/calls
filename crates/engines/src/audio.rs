//! Client for the `calls-audio` sidecar (sidecars/calls-audio): call detection and recording
//! with Apple's audio APIs, spoken over JSON lines on stdin/stdout.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::thread;

use anyhow::{Context, Result};
use calls_core::detect::AudioProcess;
use serde::Deserialize;
use serde_json::json;

use crate::sidecar;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AudioEvent {
    Ready,
    /// Sent every second.
    AudioApps {
        apps: Vec<AudioProcess>,
    },
    /// Peak levels in `0.0..=1.0`, about ten times a second while recording.
    Levels {
        mic: f32,
        system: f32,
    },
    RecordingStarted,
    /// Files that got audio (the mic one is missing if permission was denied), and when each
    /// started relative to the recording.
    RecordingStopped {
        files: Vec<String>,
        #[serde(default)]
        offsets_ms: HashMap<String, u64>,
    },
    Error {
        message: String,
    },
}

pub struct AudioDaemon {
    stdin: Mutex<ChildStdin>,
    // Kept so the process lives as long as we do. Dropping stdin makes it stop and exit.
    _child: Child,
}

impl AudioDaemon {
    pub fn spawn(on_event: impl Fn(AudioEvent) + Send + 'static) -> Result<AudioDaemon> {
        let bin = sidecar::locate("calls-audio").context("calls-audio not found (run scripts/build-sidecars.sh)")?;
        let mut child = Command::new(&bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("could not start {}", bin.display()))?;

        let stdout = child.stdout.take().expect("piped");
        thread::Builder::new().name("calls-audio".into()).spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                match serde_json::from_str(&line) {
                    Ok(event) => on_event(event),
                    Err(e) => eprintln!("calls-audio: unexpected output {line:?}: {e}"),
                }
            }
        })?;

        let stdin = child.stdin.take().expect("piped");
        Ok(AudioDaemon { stdin: Mutex::new(stdin), _child: child })
    }

    /// Records into `dir`. With a bundle prefix only that app's audio is captured as the other
    /// side; without one, everything the Mac plays.
    pub fn start(&self, dir: &Path, bundle_prefix: Option<&str>) -> Result<()> {
        self.send(json!({ "cmd": "start", "dir": dir, "bundle_prefix": bundle_prefix }))
    }

    pub fn stop(&self) -> Result<()> {
        self.send(json!({ "cmd": "stop" }))
    }

    fn send(&self, command: serde_json::Value) -> Result<()> {
        let mut stdin = self.stdin.lock().expect("poisoned");
        writeln!(stdin, "{command}").context("calls-audio is not running")?;
        stdin.flush()?;
        Ok(())
    }
}
