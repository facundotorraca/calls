use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// Metadata file stored in every call folder.
pub const CALL_FILE: &str = "call.json";

/// Identifies a call. It is also the name of the call's folder inside the library,
/// e.g. `2026-10-23_1530_acme-kickoff`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallId(pub String);

impl fmt::Display for CallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    /// Being recorded right now.
    Recording,
    /// Waiting for the pipeline to pick it up.
    Pending,
    Processing {
        step: String,
    },
    Ready,
    Failed {
        step: String,
        error: String,
    },
}

/// Where the audio of a track comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackRole {
    /// Everyone mixed together (imported recordings).
    #[default]
    Mixed,
    /// Your microphone.
    Microphone,
    /// What the call app played: the other side.
    System,
}

/// One audio stream of a call, stored inside the call folder.
///
/// Imported files produce a single mixed track. Native recordings produce one track per
/// source (mic = you, system audio = the other side), which gives speaker labels for free.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// File name relative to the call folder.
    pub file: String,
    pub speaker: Option<String>,
    #[serde(default)]
    pub role: TrackRole,
    /// When this track started relative to the call, so tracks that started late line up.
    #[serde(default)]
    pub offset_ms: u64,
}

impl Track {
    /// The 16 kHz WAV the transcriber reads, derived from the track file (`mic.m4a` -> `mic.wav`).
    pub fn wav(&self) -> String {
        let stem = Path::new(&self.file).file_stem().unwrap_or_default().to_string_lossy();
        format!("{stem}.wav")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    pub id: CallId,
    pub title: String,
    /// Folder in the library's sidebar. A label, not a directory: call folders never move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    pub created_at: DateTime<Local>,
    /// Original file the call was imported from. It is referenced, never copied or moved.
    pub source: Option<PathBuf>,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub tracks: Vec<Track>,
    pub status: Status,
    /// Steps already applied to this call, by step id. The pipeline skips them, which makes
    /// processing resumable and lets new steps run over old calls.
    #[serde(default)]
    pub completed_steps: BTreeMap<String, DateTime<Local>>,
}

impl Call {
    pub fn set_track(&mut self, track: Track) {
        self.tracks.retain(|t| t.file != track.file);
        self.tracks.push(track);
    }
}
