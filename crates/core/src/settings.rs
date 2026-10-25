use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::detect::CallApp;
use crate::notion::NotionPage;

/// User preferences, persisted as JSON in the data dir. Missing fields fall back to defaults,
/// so adding a setting never breaks an existing file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// What the person using the app is called in transcripts and summaries. Empty: "Yo".
    pub user_name: String,
    /// Where call folders are stored.
    pub library_dir: PathBuf,
    /// ISO 639-1 code, or `auto`.
    pub language: String,
    /// whisper.cpp model name, resolved to `<models dir>/ggml-<name>.bin`.
    pub whisper_model: String,
    /// Names and terms the transcriber tends to get wrong (people, clients, products,
    /// acronyms), spelled the way they should be written.
    pub vocabulary: Vec<String>,
    /// Tell the voices of a call apart (needs the pyannote model, downloaded once).
    pub diarize: bool,
    /// Voice activity detection model, used to skip silences (avoids made up text in them).
    /// Ignored if the file is not present.
    pub vad_model: Option<String>,
    /// Show the island when a call app starts using the microphone.
    pub detect_calls: bool,
    /// Start recording detected calls without asking. Off by default: ask for consent first.
    pub auto_record: bool,
    /// Apps whose microphone use counts as a call.
    pub call_apps: Vec<CallApp>,
    /// Summary, decisions and action items with an LLM after transcribing.
    pub summarize: bool,
    /// Language summaries are written in, whatever the call's language.
    pub summary_language: String,
    pub llm: LlmSettings,
    pub notion: NotionSettings,
}

/// Exporting calls to Notion. The token itself is in the Keychain, not here.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotionSettings {
    /// The workspace the token belongs to, once connected.
    pub workspace: Option<String>,
    /// Where call pages go when neither the call nor its folder has a page of its own.
    pub parent: Option<NotionPage>,
}

/// A local LLM served by Ollama.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmSettings {
    pub base_url: String,
    pub model: String,
}

impl Default for LlmSettings {
    fn default() -> Self {
        LlmSettings { base_url: "http://localhost:11434".into(), model: "qwen3.5:35b".into() }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            user_name: String::new(),
            library_dir: home_dir().join("Calls"),
            language: "auto".into(),
            whisper_model: "large-v3-turbo".into(),
            vocabulary: Vec::new(),
            diarize: true,
            vad_model: Some("silero-v6.2.0".into()),
            detect_calls: true,
            auto_record: false,
            call_apps: CallApp::defaults(),
            summarize: true,
            summary_language: "español".into(),
            llm: LlmSettings::default(),
            notion: NotionSettings::default(),
        }
    }
}

/// The speaker label of the person using the app, when they have not given their name.
pub const ME: &str = "Yo";

impl Settings {
    /// The speaker label of the person using the app: their name, or "Yo".
    pub fn me(&self) -> String {
        match self.user_name.trim() {
            "" => ME.into(),
            name => name.into(),
        }
    }

    /// The vocabulary as a prompt for the transcriber, if there is one. Whisper reads the
    /// prompt as text said just before, which nudges it towards these spellings.
    ///
    /// A sentence, not a bare list: tried on a real call, a bare list made whisper skip the
    /// first seconds of speech, while a sentence kept them and fixed the names.
    pub fn vocabulary_prompt(&self) -> Option<String> {
        let terms: Vec<&str> = self.vocabulary.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
        if terms.is_empty() {
            return None;
        }
        let mut prompt = String::from(if self.language == "es" { "En esta llamada: " } else { "In this call: " });
        // Whisper keeps at most half its context (~220 tokens) of prompt; stay well under.
        for (i, term) in terms.iter().enumerate() {
            if prompt.len() + term.len() > 600 {
                break;
            }
            if i > 0 {
                prompt.push_str(", ");
            }
            prompt.push_str(term);
        }
        prompt.push('.');
        Some(prompt)
    }

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
        model_path(&self.whisper_model)
    }

    pub fn vad_model_path(&self) -> Option<PathBuf> {
        self.vad_model.as_deref().map(model_path)
    }
}

/// App data shared by every front end (desktop, CLI): settings and models.
pub fn data_dir() -> PathBuf {
    home_dir().join("Library/Application Support/calls")
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

/// Where a whisper.cpp model (transcription or VAD) named `name` lives.
pub fn model_path(name: &str) -> PathBuf {
    models_dir().join(format!("ggml-{name}.bin"))
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

/// The voice of the person using the app, learnt from the calls (see [`crate::voices`]).
pub fn my_voice_path() -> PathBuf {
    data_dir().join("voice.json")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_prompt() {
        let mut settings = Settings::default();
        assert_eq!(settings.vocabulary_prompt(), None);
        settings.vocabulary = vec![" Acme ".into(), "".into(), "Kubernetes".into(), "Srinika".into()];
        assert_eq!(settings.vocabulary_prompt().as_deref(), Some("In this call: Acme, Kubernetes, Srinika."));
        settings.language = "es".into();
        assert_eq!(settings.vocabulary_prompt().as_deref(), Some("En esta llamada: Acme, Kubernetes, Srinika."));
        settings.vocabulary = vec!["x".repeat(400); 3];
        assert_eq!(settings.vocabulary_prompt().unwrap(), format!("En esta llamada: {}.", "x".repeat(400)));
    }
}
