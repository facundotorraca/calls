//! Built-in pipeline steps.

use std::fs;

use anyhow::{Context, Result, bail};

use crate::call::TrackRole;
use crate::engines::{AudioExtractor, Llm, TranscribeRequest, Transcriber};
use crate::summary::{SUMMARY_JSON, SUMMARY_MD, Summary};
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

/// Summary, decisions and action items (`summary.json`, `summary.md`), by an LLM.
pub struct Summarize {
    pub llm: Box<dyn Llm>,
    /// Language to write in, e.g. `español`, whatever the call's language.
    pub language: String,
}

impl Summarize {
    fn system_prompt(&self) -> String {
        format!(
            "You summarize work calls for the person who recorded them. Write in {language}.\n\
             Be concrete: names, numbers, dates and commitments as said in the call. Never add \
             anything that was not said. The transcript comes from speech recognition and may \
             contain misheard words: read them in context (a name spelled several ways is one person). \
             When speakers are labeled, \"Yo\" is the person who recorded the call.\n\
             - summary: one to three short paragraphs: why the call happened, what was discussed, how it ended.\n\
             - decisions: what was agreed or decided. Empty if nothing was.\n\
             - action_items: next steps someone committed to or was asked to do. owner: the person \
             responsible as named in the call, or null. due: when, as said in the call, or null.",
            language = self.language
        )
    }
}

impl Step for Summarize {
    fn id(&self) -> &'static str {
        "summarize"
    }

    fn run(&self, call: &mut Call, ctx: &StepContext) -> Result<()> {
        let json = fs::read_to_string(ctx.dir.join(TRANSCRIPT_JSON)).context("transcript not found")?;
        let transcript: Transcript = serde_json::from_str(&json)?;
        if transcript.segments.is_empty() {
            bail!("the transcript is empty");
        }

        let prompt = format!(
            "Call: {}\nDate: {}\n\nTranscript:\n{}",
            call.title,
            call.created_at.format("%Y-%m-%d %H:%M"),
            dialogue(&transcript)
        );
        ctx.progress(0.1);
        let answer = self.llm.json(&self.system_prompt(), &prompt, &Summary::schema())?;
        let summary: Summary =
            serde_json::from_value(answer).context("the model answered with an unexpected format")?;

        fs::write(ctx.dir.join(SUMMARY_JSON), serde_json::to_vec_pretty(&summary)?)?;
        fs::write(ctx.dir.join(SUMMARY_MD), summary.to_markdown(&call.title))?;
        Ok(())
    }
}

/// The transcript as compact text for a model: one line per speaker turn.
fn dialogue(transcript: &Transcript) -> String {
    let mut out = String::new();
    let mut current: Option<&Option<String>> = None;
    for segment in &transcript.segments {
        if current != Some(&segment.speaker) {
            if current.is_some() {
                out.push('\n');
            }
            out.push_str(&format!("[{}]", transcript::format_timestamp(segment.start_ms)));
            if let Some(speaker) = &segment.speaker {
                out.push_str(&format!(" {speaker}:"));
            }
            current = Some(&segment.speaker);
        }
        out.push(' ');
        out.push_str(&segment.text);
    }
    out
}
