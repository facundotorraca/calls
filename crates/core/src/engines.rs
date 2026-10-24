//! Contracts for the heavy lifting. Implementations live outside the core
//! (see the `calls-engines` crate), so any of them can be swapped without touching steps.

use std::path::Path;

use anyhow::Result;

use crate::Transcript;

/// Receives completion in `0.0..=1.0`.
pub type Progress<'a> = &'a dyn Fn(f32);

pub struct AudioInfo {
    pub duration_ms: Option<u64>,
}

pub trait AudioExtractor: Send + Sync {
    /// Decodes any audio or video file into 16 kHz mono PCM WAV, the format speech models expect.
    fn extract(&self, input: &Path, output: &Path, progress: Progress) -> Result<AudioInfo>;
}

pub struct TranscribeRequest<'a> {
    pub audio: &'a Path,
    /// ISO 639-1 code, or `auto` to let the model detect it.
    pub language: &'a str,
}

pub trait Transcriber: Send + Sync {
    fn transcribe(&self, request: &TranscribeRequest, progress: Progress) -> Result<Transcript>;
}
