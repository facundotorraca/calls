//! Contracts for the heavy lifting. Implementations live outside the core
//! (see the `calls-engines` crate), so any of them can be swapped without touching steps.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::Transcript;

/// Receives completion in `0.0..=1.0`.
pub type Progress<'a> = &'a dyn Fn(f32);

pub struct AudioInfo {
    pub duration_ms: Option<u64>,
}

/// One input of a mix: a file, and when it starts relative to the call.
pub struct MixInput<'a> {
    pub file: &'a Path,
    pub offset_ms: u64,
}

pub trait AudioExtractor: Send + Sync {
    /// Decodes any audio or video file into 16 kHz mono PCM WAV, the format speech models expect.
    fn extract(&self, input: &Path, output: &Path, progress: Progress) -> Result<AudioInfo>;

    /// Mixes the inputs, each delayed by its offset, into one compressed file for listening
    /// (AAC in `.m4a`).
    fn mix(&self, inputs: &[MixInput], output: &Path, progress: Progress) -> Result<()>;
}

pub struct TranscribeRequest<'a> {
    pub audio: &'a Path,
    /// ISO 639-1 code, or `auto` to let the model detect it.
    pub language: &'a str,
    /// Text that steers the spelling of names and terms (see `Settings::vocabulary_prompt`).
    pub prompt: Option<&'a str>,
}

pub trait Transcriber: Send + Sync {
    fn transcribe(&self, request: &TranscribeRequest, progress: Progress) -> Result<Transcript>;
}

/// A stretch of audio and who was speaking in it, as a diarizer labels them (`SPEAKER_00`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpeakerTurn {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: String,
}

/// Who spoke when, and what each voice sounds like.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Diarization {
    pub turns: Vec<SpeakerTurn>,
    /// A fingerprint (embedding) per speaker of `turns`, to recognize them elsewhere.
    #[serde(default)]
    pub voices: BTreeMap<String, Vec<f32>>,
}

/// Tells voices apart: who spoke when, without knowing who they are.
pub trait Diarizer: Send + Sync {
    /// Takes a 16 kHz mono WAV, like the transcriber.
    fn diarize(&self, audio: &Path, progress: Progress) -> Result<Diarization>;
}

/// A language model, local or remote.
pub trait Llm: Send + Sync {
    /// Asks for an answer as a JSON value matching `schema` (a JSON Schema object).
    fn json(&self, system: &str, prompt: &str, schema: &serde_json::Value) -> Result<serde_json::Value>;

    /// Whether loading the model now would leave the computer short of memory. `None` if
    /// it fits, is already loaded, runs elsewhere or that cannot be told.
    fn memory_shortage(&self) -> Option<MemoryShortage> {
        None
    }
}

/// A model that does not fit in the memory free right now, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MemoryShortage {
    pub needed: u64,
    pub available: u64,
}
