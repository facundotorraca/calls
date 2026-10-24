//! Speaker diarization with pyannote (community-1), through the `diarize` Python worker.
//! The model is gated on Hugging Face: downloading it once needs a token, running it does not.

use std::path::PathBuf;

use anyhow::Result;
use calls_core::engines::{Diarizer, Progress, SpeakerTurn};
use calls_core::settings;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::python::PythonWorker;

/// Where Hugging Face keeps the model: inside the app's data, not in `~/.cache`.
fn hf_home() -> PathBuf {
    settings::data_dir().join("huggingface")
}

fn worker() -> Result<PythonWorker> {
    Ok(PythonWorker::locate("diarize")?.env("HF_HOME", hf_home()))
}

/// Whether the model is on disk. Its config is what counts: a refused download (gated, not
/// accepted) still leaves the public README behind.
pub fn is_downloaded() -> bool {
    let snapshots = hf_home().join("hub/models--pyannote--speaker-diarization-community-1/snapshots");
    snapshots
        .read_dir()
        .is_ok_and(|entries| entries.flatten().any(|snapshot| snapshot.path().join("config.yaml").exists()))
}

/// Downloads the model with a Hugging Face token. The first run also installs the
/// worker's environment (PyTorch, ~1 GB), so it takes a while.
pub fn download(token: &str, progress: Progress) -> Result<()> {
    worker()?.run::<Value>(&json!({ "action": "download", "token": token }), progress)?;
    Ok(())
}

pub struct Pyannote {
    worker: PythonWorker,
}

impl Pyannote {
    pub fn locate() -> Result<Pyannote> {
        // Offline: once downloaded, nothing about a call leaves the machine.
        Ok(Pyannote { worker: worker()?.env("HF_HUB_OFFLINE", "1") })
    }
}

impl Diarizer for Pyannote {
    fn diarize(&self, audio: &std::path::Path, progress: Progress) -> Result<Vec<SpeakerTurn>> {
        #[derive(Deserialize)]
        struct Output {
            turns: Vec<SpeakerTurn>,
        }
        let output: Output = self.worker.run(&json!({ "audio": audio }), progress)?;
        Ok(output.turns)
    }
}
