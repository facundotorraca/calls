//! Implementations of the core engine traits, backed by external binaries run as child
//! processes. We depend on the tools' command line interface (stable, official) instead of
//! linking third-party bindings.

pub mod audio;
mod ffmpeg;
pub mod keychain;
mod memory;
pub mod models;
pub mod notion;
pub mod ollama;
mod process;
pub mod pyannote;
pub mod python;
pub mod sidecar;
pub mod trash;
mod whisper;

use std::path::PathBuf;

use anyhow::Result;
use calls_core::Pipeline;
use calls_core::Settings;
use calls_core::settings::my_voice_path;
use calls_core::steps::{Diarize, ExtractAudio, MixPlayback, NameSpeakers, RenderMarkdown, Summarize, Transcribe};
use calls_core::voices::MyVoice;
use serde::Serialize;

pub use ffmpeg::Ffmpeg;
pub use ollama::Ollama;
pub use whisper::WhisperCli;

/// The default processing pipeline, wired with the real engines.
/// This is the composition root: the only place that knows which engine backs which step.
///
/// An optional step that is off, or whose engine is not there (the model not downloaded,
/// Ollama closed), is left out: nothing to fail, and `Doctor` says what is missing.
pub fn default_pipeline(settings: &Settings) -> Result<Pipeline> {
    let ffmpeg = Ffmpeg::locate()?;
    let whisper = WhisperCli::new(settings.whisper_model_path(), settings.vad_model_path());

    let mut pipeline = Pipeline::new()
        .with(ExtractAudio { extractor: Box::new(ffmpeg.clone()) })
        // Quick, and first: the call can be listened to whatever happens with the rest.
        .with(MixPlayback { mixer: Box::new(ffmpeg) })
        .with(Transcribe {
            skips_silences: whisper.skips_silences(),
            transcriber: Box::new(whisper),
            language: settings.language.clone(),
            prompt: settings.vocabulary_prompt(),
        });
    let diarizer = if settings.diarize && pyannote::is_downloaded() { pyannote::Pyannote::locate().ok() } else { None };
    let diarize = diarizer.is_some();
    if let Some(diarizer) = diarizer {
        pipeline = pipeline.with(Diarize {
            diarizer: Box::new(diarizer),
            me: settings.me(),
            my_voice: MyVoice::load(&my_voice_path()),
        });
    }
    pipeline = pipeline.with(RenderMarkdown);

    // Last: the transcript is already usable, and the model takes long.
    if settings.summarize && Ollama::new(&settings.llm.base_url, &settings.llm.model)?.check().is_ok() {
        let llm = || Ollama::new(&settings.llm.base_url, &settings.llm.model);
        // Names first, so the summary can say who said what.
        if diarize {
            pipeline = pipeline.with(NameSpeakers { llm: Box::new(llm()?), me: settings.me() });
        }
        let vocabulary = settings.vocabulary.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).map(Into::into);
        pipeline = pipeline.with(Summarize {
            llm: Box::new(llm()?),
            me: settings.me(),
            language: settings.summary_language.clone(),
            vocabulary: vocabulary.collect(),
        });
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
    /// Voices get told apart: turned on and the model downloaded.
    pub diarization: bool,
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
            diarization: settings.diarize && pyannote::is_downloaded(),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ffmpeg.is_some() && self.whisper_cli.is_some() && self.whisper_model_present
    }
}
