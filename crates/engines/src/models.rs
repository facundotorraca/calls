//! whisper.cpp models on disk: which ones the app offers, which are downloaded, and the
//! download itself. Files come from the official whisper.cpp repositories on Hugging Face,
//! the same ones `vendor/whisper.cpp/models/download-*-model.sh` use.

use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use calls_core::settings::{model_path, models_dir};
use reqwest::blocking::Client;
use serde::Serialize;

/// A transcription model the app offers to download.
struct Entry {
    name: &'static str,
    /// File size, from the Hugging Face repository.
    bytes: u64,
    note: &'static str,
}

/// Multilingual models only: calls mix languages. Best first.
const WHISPER_CATALOG: &[Entry] = &[
    Entry {
        name: "large-v3-turbo",
        bytes: 1_624_555_275,
        note: "Recomendado: calidad de large-v3, bastante más rápido",
    },
    Entry { name: "large-v3", bytes: 3_095_033_483, note: "La mejor calidad, el más lento" },
    Entry {
        name: "large-v3-turbo-q8_0", bytes: 874_188_075, note: "Turbo cuantizado: casi igual, la mitad de tamaño"
    },
    Entry {
        name: "large-v3-turbo-q5_0",
        bytes: 574_041_195,
        note: "Turbo cuantizado: más liviano, algo menos preciso",
    },
    Entry { name: "large-v3-q5_0", bytes: 1_081_140_203, note: "large-v3 cuantizado" },
    Entry { name: "medium", bytes: 1_533_763_059, note: "Bueno y rápido" },
    Entry { name: "small", bytes: 487_601_967, note: "Rápido, para pruebas" },
    Entry { name: "base", bytes: 147_951_465, note: "Muy rápido, poco preciso" },
    Entry { name: "tiny", bytes: 77_691_713, note: "Solo para probar que todo funciona" },
];

/// Voice activity detection models, used to skip silences.
const VAD_CATALOG: &[Entry] = &[Entry { name: "silero-v6.2.0", bytes: 885_098, note: "Detecta silencios" }];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Whisper,
    Vad,
}

impl ModelKind {
    fn catalog(self) -> &'static [Entry] {
        match self {
            ModelKind::Whisper => WHISPER_CATALOG,
            ModelKind::Vad => VAD_CATALOG,
        }
    }

    fn url(self, name: &str) -> String {
        let repo = match self {
            ModelKind::Whisper => "ggerganov/whisper.cpp",
            ModelKind::Vad => "ggml-org/whisper-vad",
        };
        format!("https://huggingface.co/{repo}/resolve/main/ggml-{name}.bin")
    }

    fn owns(self, name: &str) -> bool {
        let vad = name.starts_with("silero");
        match self {
            ModelKind::Whisper => !vad,
            ModelKind::Vad => vad,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalModel {
    pub name: String,
    pub note: Option<String>,
    /// On disk if downloaded, otherwise the size of the download.
    pub bytes: Option<u64>,
    pub downloaded: bool,
}

/// The catalog, plus models of this kind found in the models dir that it does not list
/// (downloaded by hand or with the script).
pub fn list(kind: ModelKind) -> Vec<LocalModel> {
    let size = |name: &str| fs::metadata(model_path(name)).ok().filter(|m| m.is_file()).map(|m| m.len());
    let mut models: Vec<LocalModel> = kind
        .catalog()
        .iter()
        .map(|entry| {
            let on_disk = size(entry.name);
            LocalModel {
                name: entry.name.into(),
                note: Some(entry.note.into()),
                bytes: on_disk.or(Some(entry.bytes)),
                downloaded: on_disk.is_some(),
            }
        })
        .collect();

    let files = fs::read_dir(models_dir()).into_iter().flatten().flatten();
    for file in files {
        let file_name = file.file_name().to_string_lossy().into_owned();
        let Some(name) = file_name.strip_prefix("ggml-").and_then(|n| n.strip_suffix(".bin")) else { continue };
        if kind.owns(name) && !models.iter().any(|m| m.name == name) {
            models.push(LocalModel { name: name.into(), note: None, bytes: size(name), downloaded: true });
        }
    }
    models
}

/// Downloads a model from the catalog into the models dir. Writes to a `.part` file first,
/// so an interrupted download never looks like a model.
pub fn download(kind: ModelKind, name: &str, progress: &dyn Fn(f32), cancelled: &AtomicBool) -> Result<()> {
    if !kind.catalog().iter().any(|e| e.name == name) {
        bail!("unknown model {name}");
    }
    let dest = model_path(name);
    fs::create_dir_all(models_dir())?;
    let part = dest.with_extension("bin.part");
    let result = fetch(&kind.url(name), &part, progress, cancelled);
    match result {
        Ok(()) => fs::rename(&part, &dest).with_context(|| format!("saving {}", dest.display())),
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

fn fetch(url: &str, dest: &Path, progress: &dyn Fn(f32), cancelled: &AtomicBool) -> Result<()> {
    // No overall timeout: a 3 GB model takes as long as it takes.
    let client = Client::builder().connect_timeout(Duration::from_secs(15)).timeout(None).build()?;
    let mut response = client.get(url).send().with_context(|| format!("downloading {url}"))?.error_for_status()?;
    let total = response.content_length();
    let mut out = BufWriter::new(File::create(dest)?);
    let mut buf = vec![0; 1 << 16];
    let (mut done, mut reported) = (0u64, 0.0f32);
    loop {
        if cancelled.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        let n = response.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        done += n as u64;
        if let Some(total) = total {
            let fraction = done as f32 / total as f32;
            // Every half percent is plenty for a progress bar.
            if fraction - reported >= 0.005 {
                reported = fraction;
                progress(fraction);
            }
        }
    }
    out.flush()?;
    if total.is_some_and(|total| total != done) {
        bail!("the download was cut short");
    }
    Ok(())
}

/// Deletes a downloaded model.
pub fn delete(name: &str) -> Result<()> {
    // A name, not a path: nothing outside the models dir.
    if name.is_empty() || name.starts_with('.') || !name.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c))
    {
        bail!("invalid model name {name}");
    }
    let path = model_path(name);
    fs::remove_file(&path).with_context(|| format!("deleting {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_split_the_models_dir() {
        assert!(ModelKind::Whisper.owns("large-v3-turbo"));
        assert!(!ModelKind::Whisper.owns("silero-v6.2.0"));
        assert!(ModelKind::Vad.owns("silero-v6.2.0"));
        assert_eq!(
            ModelKind::Vad.url("silero-v6.2.0"),
            "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin"
        );
    }

    #[test]
    fn only_catalog_models_download() {
        let err = download(ModelKind::Whisper, "../../etc", &|_| {}, &AtomicBool::new(false)).unwrap_err();
        assert!(err.to_string().contains("unknown model"));
    }
}
