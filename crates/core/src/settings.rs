use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::detect::CallApp;

/// User preferences, persisted as JSON in the data dir. Missing fields fall back to defaults,
/// so adding a setting never breaks an existing file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where call folders are stored.
    pub library_dir: PathBuf,
    /// ISO 639-1 code, or `auto`.
    pub language: String,
    /// whisper.cpp model name, resolved to `<models dir>/ggml-<name>.bin`.
    pub whisper_model: String,
    /// Voice activity detection model, used to skip silences (avoids made up text in them).
    /// Ignored if the file is not present.
    pub vad_model: Option<String>,
    /// Show the island when a call app starts using the microphone.
    pub detect_calls: bool,
    /// Start recording detected calls without asking. Off by default: ask for consent first.
    pub auto_record: bool,
    /// Apps whose microphone use counts as a call.
    pub call_apps: Vec<CallApp>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            library_dir: home_dir().join("Calls"),
            language: "auto".into(),
            whisper_model: "large-v3-turbo".into(),
            vad_model: Some("silero-v6.2.0".into()),
            detect_calls: true,
            auto_record: false,
            call_apps: CallApp::defaults(),
        }
    }
}

impl Settings {
    pub fn load_or_default(path: &Path) -> Result<Settings> {
        match fs::read_to_string(path) {
            Ok(json) => serde_json::from_str(&json).with_context(|| format!("invalid settings in {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn whisper_model_path(&self) -> PathBuf {
        models_dir().join(format!("ggml-{}.bin", self.whisper_model))
    }

    pub fn vad_model_path(&self) -> Option<PathBuf> {
        self.vad_model.as_ref().map(|name| models_dir().join(format!("ggml-{name}.bin")))
    }
}

/// App data shared by every front end (desktop, CLI): settings and models.
pub fn data_dir() -> PathBuf {
    home_dir().join("Library/Application Support/calls")
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}
