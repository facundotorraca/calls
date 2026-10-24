//! Implementations of the core engine traits, backed by external binaries run as child
//! processes. We depend on the tools' command line interface (stable, official) instead of
//! linking third-party bindings.

pub mod audio;
mod ffmpeg;
pub mod models;
pub mod ollama;
mod process;
pub mod python;
pub mod sidecar;
mod whisper;

use std::path::PathBuf;

use anyhow::Result;
use calls_core::Pipeline;
use calls_core::Settings;
use calls_core::steps::{ExtractAudio, RenderMarkdown, Summarize, Transcribe};
use serde::Serialize;

pub use ffmpeg::Ffmpeg;
pub use ollama::Ollama;
pub use whisper::WhisperCli;

/// The default processing pipeline, wired with the real engines.
/// This is the composition root: the only place that knows which engine backs which step.
pub fn default_pipeline(settings: &Settings) -> Result<Pipeline> {
    let ffmpeg = Ffmpeg::locate()?;
    let whisper = WhisperCli::locate(settings.whisper_model_path(), settings.vad_model_path())?;

    let mut pipeline = Pipeline::new()
        .with(ExtractAudio { extractor: Box::new(ffmpeg) })
        .with(Transcribe { transcriber: Box::new(whisper), language: settings.language.clone() })
        .with(RenderMarkdown);
    // Last: the transcript is already usable if the LLM is not available.
    if settings.summarize {
        let llm = Ollama::new(&settings.llm.base_url, &settings.llm.model)?;
        pipeline = pipeline.with(Summarize { llm: Box::new(llm), language: settings.summary_language.clone() });
    }
    Ok(pipeline)
}

/// What is installed and what is missing, so front ends can guide the user through setup.
#[derive(Debug, Clone, Serialize)]
pub struct Doctor {
    pub ffmpeg: Option<PathBuf>,
    pub whisper_cli: Option<PathBuf>,
    /// Recording and call detection. Optional: importing files works without it.
    pub calls_audio: Option<PathBuf>,
    pub whisper_model: PathBuf,
    pub whisper_model_present: bool,
    pub vad_model_present: bool,
    pub library_dir: PathBuf,
    /// Python workers.
    pub uv: Option<PathBuf>,
    /// Why the LLM is not usable, if it is not (Ollama closed, model missing).
    pub llm_error: Option<String>,
}

impl Doctor {
    pub fn check(settings: &Settings) -> Doctor {
        let whisper_model = settings.whisper_model_path();
        Doctor {
            ffmpeg: sidecar::locate("ffmpeg"),
            whisper_cli: sidecar::locate("whisper-cli"),
            calls_audio: sidecar::locate("calls-audio"),
            whisper_model_present: whisper_model.is_file(),
            whisper_model,
            vad_model_present: settings.vad_model_path().is_some_and(|p| p.is_file()),
            library_dir: settings.library_dir.clone(),
            uv: sidecar::locate("uv"),
            // Summaries turned off: nothing to report about the LLM.
            llm_error: settings
                .summarize
                .then(|| Ollama::new(&settings.llm.base_url, &settings.llm.model).and_then(|llm| llm.check()).err())
                .flatten()
                .map(|e| format!("{e:#}")),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ffmpeg.is_some() && self.whisper_cli.is_some() && self.whisper_model_present
    }
}
