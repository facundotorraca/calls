use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use calls_core::engines::{Progress, TranscribeRequest, Transcriber};
use calls_core::{Segment, Transcript};
use serde::Deserialize;

use crate::process::{Stream, run_lines};
use crate::sidecar;

/// Transcribes with whisper.cpp's own `whisper-cli`, built from `vendor/whisper.cpp`.
pub struct WhisperCli {
    bin: PathBuf,
    model: PathBuf,
    vad_model: Option<PathBuf>,
}

impl WhisperCli {
    pub fn locate(model: PathBuf, vad_model: Option<PathBuf>) -> Result<WhisperCli> {
        let bin = sidecar::locate("whisper-cli").context("whisper-cli not found (run scripts/build-sidecars.sh)")?;
        if !model.is_file() {
            bail!("whisper model not found at {} (run scripts/download-models.sh)", model.display());
        }
        Ok(WhisperCli { bin, model, vad_model: vad_model.filter(|p| p.is_file()) })
    }
}

impl Transcriber for WhisperCli {
    fn transcribe(&self, request: &TranscribeRequest, progress: Progress) -> Result<Transcript> {
        // whisper-cli appends `.json` to the output prefix.
        let prefix = request.audio.with_extension("whisper");
        let json_path = prefix.with_extension("whisper.json");

        let mut cmd = Command::new(&self.bin);
        cmd.arg("--model")
            .arg(&self.model)
            .arg("--file")
            .arg(request.audio)
            .args(["--language", request.language])
            .args(["--output-json", "--print-progress", "--output-file"])
            .arg(&prefix);
        if let Some(vad) = &self.vad_model {
            cmd.arg("--vad").arg("--vad-model").arg(vad);
        }

        run_lines(cmd, Stream::Stderr, |line| {
            // e.g. "whisper_print_progress_callback: progress =  35%"
            if let Some((_, pct)) = line.split_once("progress =")
                && let Ok(pct) = pct.trim().trim_end_matches('%').parse::<f32>()
            {
                progress(pct / 100.0);
            }
        })?;

        let json =
            fs::read_to_string(&json_path).with_context(|| format!("whisper-cli wrote no {}", json_path.display()))?;
        let _ = fs::remove_file(&json_path);
        parse(&json)
    }
}

#[derive(Deserialize)]
struct Output {
    result: OutputResult,
    transcription: Vec<OutputSegment>,
}

#[derive(Deserialize)]
struct OutputResult {
    language: String,
}

#[derive(Deserialize)]
struct OutputSegment {
    offsets: Offsets,
    text: String,
}

/// Milliseconds from the start of the audio.
#[derive(Deserialize)]
struct Offsets {
    from: u64,
    to: u64,
}

fn parse(json: &str) -> Result<Transcript> {
    let output: Output = serde_json::from_str(json).context("unexpected whisper-cli output")?;
    let segments = output
        .transcription
        .into_iter()
        .map(|s| Segment {
            start_ms: s.offsets.from,
            end_ms: s.offsets.to,
            text: s.text.trim().to_string(),
            speaker: None,
        })
        .filter(|s| !s.text.is_empty())
        .collect();
    Ok(Transcript { language: output.result.language, segments })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_whisper_cli_json() {
        let json = r#"{
            "systeminfo": "...", "model": {}, "params": {},
            "result": { "language": "es" },
            "transcription": [
                { "timestamps": { "from": "00:00:00,000", "to": "00:00:02,500" },
                  "offsets": { "from": 0, "to": 2500 }, "text": " Hola, ¿cómo andás?" },
                { "timestamps": { "from": "00:00:02,500", "to": "00:00:03,000" },
                  "offsets": { "from": 2500, "to": 3000 }, "text": " " }
            ]
        }"#;

        let transcript = parse(json).unwrap();

        assert_eq!(transcript.language, "es");
        assert_eq!(transcript.segments.len(), 1);
        assert_eq!(transcript.segments[0].text, "Hola, ¿cómo andás?");
        assert_eq!(transcript.segments[0].end_ms, 2500);
    }
}
