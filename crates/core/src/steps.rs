//! Built-in pipeline steps.

use std::fs;

use anyhow::{Context, Result, bail};

use crate::call::TrackRole;
use crate::engines::{AudioExtractor, TranscribeRequest, Transcriber};
use crate::transcript::{self, TRANSCRIPT_JSON, TRANSCRIPT_MD};
use crate::{Call, Step, StepContext, Track, Transcript};

const AUDIO_FILE: &str = "audio.wav";

/// Turns the call's audio (an imported file in any audio/video format, or the recorded
/// tracks) into the 16 kHz WAV files the transcriber reads.
pub struct ExtractAudio {
    pub extractor: Box<dyn AudioExtractor>,
}

impl Step for ExtractAudio {
    fn id(&self) -> &'static str {
        "extract_audio"
    }

    fn run(&self, call: &mut Call, ctx: &StepContext) -> Result<()> {
        if let Some(source) = &call.source {
            if !source.exists() {
                bail!("the original file is gone: {}", source.display());
            }
            let info = self.extractor.extract(source, &ctx.dir.join(AUDIO_FILE), &|f| ctx.progress(f))?;
            call.duration_ms = info.duration_ms.or(call.duration_ms);
            call.set_track(Track { file: AUDIO_FILE.into(), speaker: None, role: TrackRole::Mixed, offset_ms: 0 });
            return Ok(());
        }

        if call.tracks.is_empty() {
            bail!("the recording has no audio");
        }
        let total = call.tracks.len() as f32;
        for (i, track) in call.tracks.iter().enumerate() {
            let info = self
                .extractor
                .extract(&ctx.dir.join(&track.file), &ctx.dir.join(track.wav()), &|f| {
                    ctx.progress((i as f32 + f) / total)
                })
                .with_context(|| format!("reading {}", track.file))?;
            call.duration_ms = call.duration_ms.max(info.duration_ms);
        }
        Ok(())
    }
}

/// Transcribes every track and interleaves the results into `transcript.json`.
pub struct Transcribe {
    pub transcriber: Box<dyn Transcriber>,
    pub language: String,
}

impl Step for Transcribe {
    fn id(&self) -> &'static str {
        "transcribe"
    }

    fn run(&self, call: &mut Call, ctx: &StepContext) -> Result<()> {
        if call.tracks.is_empty() {
            bail!("the call has no audio to transcribe");
        }

        let total = call.tracks.len() as f32;
        let mut parts = Vec::new();
        for (i, track) in call.tracks.iter().enumerate() {
            let request = TranscribeRequest { audio: &ctx.dir.join(track.wav()), language: &self.language };
            let mut part = self
                .transcriber
                .transcribe(&request, &|f| ctx.progress((i as f32 + f) / total))
                .with_context(|| format!("transcribing {}", track.file))?;
            for segment in &mut part.segments {
                segment.speaker = track.speaker.clone();
                segment.start_ms += track.offset_ms;
                segment.end_ms += track.offset_ms;
            }
            parts.push((track.role, part));
        }

        let system: Vec<Transcript> =
            parts.iter().filter(|(role, _)| *role == TrackRole::System).map(|(_, t)| t.clone()).collect();
        for (role, part) in &mut parts {
            if *role == TrackRole::Microphone {
                for other_side in &system {
                    transcript::remove_echo(part, other_side);
                }
            }
        }

        let transcript = Transcript::merge(parts.into_iter().map(|(_, t)| t).collect());
        fs::write(ctx.dir.join(TRANSCRIPT_JSON), serde_json::to_vec_pretty(&transcript)?)?;
        Ok(())
    }
}

/// Writes `transcript.md`, a readable version of the transcript.
pub struct RenderMarkdown;

impl Step for RenderMarkdown {
    fn id(&self) -> &'static str {
        "render_markdown"
    }

    fn run(&self, call: &mut Call, ctx: &StepContext) -> Result<()> {
        let json = fs::read_to_string(ctx.dir.join(TRANSCRIPT_JSON)).context("transcript not found")?;
        let transcript: Transcript = serde_json::from_str(&json)?;
        fs::write(ctx.dir.join(TRANSCRIPT_MD), transcript.to_markdown(&call.title))?;
        Ok(())
    }
}
