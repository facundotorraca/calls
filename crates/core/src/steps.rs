//! Built-in pipeline steps.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::call::TrackRole;
use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;
use serde_json::json;

use crate::engines::{AudioExtractor, Diarizer, Llm, MixInput, Progress, SpeakerTurn, TranscribeRequest, Transcriber};
use crate::summary::{SUMMARY_JSON, SUMMARY_MD, Summary};
use crate::transcript::{self, TRANSCRIPT_JSON, TRANSCRIPT_MD};
use crate::{Call, Step, StepContext, Track, Transcript};

const AUDIO_FILE: &str = "audio.wav";

/// What the player plays: the whole call in one file.
pub const PLAYBACK_FILE: &str = "playback.m4a";

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
    pub prompt: Option<String>,
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
            let request = TranscribeRequest {
                audio: &ctx.dir.join(track.wav()),
                language: &self.language,
                prompt: self.prompt.as_deref(),
            };
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

/// Tells the voices in the call apart. An imported recording is one mix of everyone; a
/// native one already knows "Yo" (the microphone), so only the other side is split.
pub struct Diarize {
    pub diarizer: Box<dyn Diarizer>,
}

/// What an unidentified voice is called, numbered in order of appearance.
pub const SPEAKER_PREFIX: &str = "Hablante";

impl Step for Diarize {
    fn id(&self) -> &'static str {
        "diarize"
    }

    fn run(&self, call: &mut Call, ctx: &StepContext) -> Result<()> {
        let mut transcript = load_transcript(ctx.dir)?;
        let tracks: Vec<&Track> = call.tracks.iter().filter(|t| t.role != TrackRole::Microphone).collect();
        let mut next_number = 1;
        for (i, track) in tracks.iter().enumerate() {
            let total = tracks.len() as f32;
            let turns = self
                .diarizer
                .diarize(&ctx.dir.join(track.wav()), &|f| ctx.progress((i as f32 + f) / total))
                .with_context(|| format!("diarizing {}", track.file))?;
            let shifted: Vec<SpeakerTurn> = turns
                .into_iter()
                .map(|t| SpeakerTurn {
                    start_ms: t.start_ms + track.offset_ms,
                    end_ms: t.end_ms + track.offset_ms,
                    ..t
                })
                .collect();
            next_number = assign_speakers(&mut transcript, track.speaker.as_deref(), &shifted, next_number);
        }
        save_transcript(ctx.dir, &call.title, &transcript)
    }
}

/// Gives each segment that came from the track labelled `track_speaker` the voice it
/// overlaps the most, named `Hablante 1`, `Hablante 2`... from `first_number` on, in
/// order of appearance. A track where only one voice is heard keeps its label: one voice
/// is no news. Returns the next free number.
pub fn assign_speakers(
    transcript: &mut Transcript,
    track_speaker: Option<&str>,
    turns: &[SpeakerTurn],
    first_number: usize,
) -> usize {
    if turns.is_empty() {
        return first_number;
    }
    let from_track: Vec<usize> = (0..transcript.segments.len())
        .filter(|&i| transcript.segments[i].speaker.as_deref() == track_speaker)
        .collect();
    let voices: Vec<&str> = from_track
        .iter()
        .map(|&i| {
            let segment = &transcript.segments[i];
            dominant_voice(segment.start_ms, segment.end_ms, turns)
        })
        .collect();

    let mut names: HashMap<&str, String> = HashMap::new();
    for voice in &voices {
        if !names.contains_key(voice) {
            names.insert(voice, format!("{SPEAKER_PREFIX} {}", first_number + names.len()));
        }
    }
    if names.len() < 2 {
        return first_number;
    }
    for (&i, voice) in from_track.iter().zip(&voices) {
        transcript.segments[i].speaker = Some(names[voice].clone());
    }
    first_number + names.len()
}

/// The voice that speaks the most within `start..end`, or the nearest one if none does.
fn dominant_voice(start: u64, end: u64, turns: &[SpeakerTurn]) -> &str {
    let mut overlap: BTreeMap<&str, u64> = BTreeMap::new();
    for turn in turns {
        let shared = end.min(turn.end_ms).saturating_sub(start.max(turn.start_ms));
        if shared > 0 {
            *overlap.entry(&turn.speaker).or_default() += shared;
        }
    }
    if let Some((voice, _)) = overlap.into_iter().max_by_key(|&(_, ms)| ms) {
        return voice;
    }
    let distance = |t: &SpeakerTurn| if t.end_ms < start { start - t.end_ms } else { t.start_ms.saturating_sub(end) };
    &turns.iter().min_by_key(|t| distance(t)).expect("turns is not empty").speaker
}

/// Puts names on the voices `Diarize` numbered, when the call itself says who they are
/// ("thanks, María"). Never guesses: a voice the call does not name keeps its number.
pub struct NameSpeakers {
    pub llm: Box<dyn Llm>,
}

impl Step for NameSpeakers {
    fn id(&self) -> &'static str {
        "name_speakers"
    }

    fn run(&self, call: &mut Call, ctx: &StepContext) -> Result<()> {
        let mut transcript = load_transcript(ctx.dir)?;
        let unnamed: Vec<String> =
            speakers(&transcript).into_iter().filter(|s| s.starts_with(SPEAKER_PREFIX)).collect();
        if unnamed.is_empty() {
            return Ok(());
        }

        let system = "You identify the speakers of a call transcript. Voices labelled like \
            \"Hablante 1\" are anonymous; \"Yo\" is the person who recorded the call. For each \
            anonymous label, give the person's name only when the transcript makes it clear: they \
            introduce themselves, someone addresses them by name and they answer, or they are \
            otherwise unmistakably identified. Otherwise answer null. Never guess, and never give \
            two labels the same name. Use the name as it is said (first name is fine).";
        let prompt = format!("Labels to identify: {}\n\nTranscript:\n{}", unnamed.join(", "), dialogue(&transcript));
        let schema = json!({
            "type": "object",
            "properties": {
                "speakers": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": { "label": { "type": "string" }, "name": { "type": ["string", "null"] } },
                        "required": ["label", "name"]
                    }
                }
            },
            "required": ["speakers"]
        });
        ctx.progress(0.1);

        #[derive(Deserialize)]
        struct Answer {
            speakers: Vec<Named>,
        }
        #[derive(Deserialize)]
        struct Named {
            label: String,
            name: Option<String>,
        }
        let answer: Answer = serde_json::from_value(self.llm.json(system, &prompt, &schema)?)
            .context("the model answered with an unexpected format")?;

        let taken: Vec<String> = speakers(&transcript);
        let mut renames: Vec<(String, String)> = Vec::new();
        for Named { label, name } in answer.speakers {
            let Some(name) = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) else { continue };
            let clash = taken.contains(&name) || renames.iter().any(|(_, n)| n.eq_ignore_ascii_case(&name));
            if unnamed.contains(&label) && !clash {
                renames.push((label, name));
            }
        }
        for (label, name) in &renames {
            rename_speaker(&mut transcript, label, name);
        }
        save_transcript(ctx.dir, &call.title, &transcript)
    }
}

/// Every speaker label, in order of appearance.
pub fn speakers(transcript: &Transcript) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for speaker in transcript.segments.iter().filter_map(|s| s.speaker.as_ref()) {
        if !out.contains(speaker) {
            out.push(speaker.clone());
        }
    }
    out
}

pub fn rename_speaker(transcript: &mut Transcript, from: &str, to: &str) {
    for segment in &mut transcript.segments {
        if segment.speaker.as_deref() == Some(from) {
            segment.speaker = Some(to.to_string());
        }
    }
}

fn load_transcript(dir: &Path) -> Result<Transcript> {
    let json = fs::read_to_string(dir.join(TRANSCRIPT_JSON)).context("transcript not found")?;
    Ok(serde_json::from_str(&json)?)
}

/// Writes the transcript and its readable version together, so they never disagree.
pub fn save_transcript(dir: &Path, title: &str, transcript: &Transcript) -> Result<()> {
    fs::write(dir.join(TRANSCRIPT_JSON), serde_json::to_vec_pretty(transcript)?)?;
    fs::write(dir.join(TRANSCRIPT_MD), transcript.to_markdown(title))?;
    Ok(())
}

/// Mixes the call's audio into `playback.m4a`, to listen to it along the transcript.
pub struct MixPlayback {
    pub mixer: Box<dyn AudioExtractor>,
}

impl Step for MixPlayback {
    fn id(&self) -> &'static str {
        "playback"
    }

    fn run(&self, call: &mut Call, ctx: &StepContext) -> Result<()> {
        make_playback(call, ctx.dir, self.mixer.as_ref(), &|f| ctx.progress(f))?;
        Ok(())
    }
}

/// Writes `playback.m4a` and returns its path. Also used on demand, for calls processed
/// before the player existed.
pub fn make_playback(call: &Call, dir: &Path, mixer: &dyn AudioExtractor, progress: Progress) -> Result<PathBuf> {
    // An imported recording plays from the original: the extracted WAV is 16 kHz mono, made
    // for the model, not for listening.
    let files: Vec<(PathBuf, u64)> = match call.source.as_deref().filter(|s| s.exists()) {
        Some(source) => vec![(source.to_path_buf(), 0)],
        None => call.tracks.iter().map(|t| (dir.join(&t.file), t.offset_ms)).collect(),
    };
    if files.is_empty() {
        bail!("the call has no audio");
    }
    let inputs: Vec<MixInput> =
        files.iter().map(|(file, offset_ms)| MixInput { file, offset_ms: *offset_ms }).collect();

    // Mixed aside and then moved, so the player never finds a half written file. Named per
    // process and moment: the pipeline and an on demand request may both be at it.
    let output = dir.join(PLAYBACK_FILE);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let partial = dir.join(format!("playback.{}-{nanos}.tmp.m4a", std::process::id()));
    if let Err(e) = mixer.mix(&inputs, &partial, progress) {
        let _ = fs::remove_file(&partial);
        return Err(e);
    }
    fs::rename(&partial, &output)?;
    Ok(output)
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
    /// Names and terms as they are spelled, to fix what speech recognition misheard.
    pub vocabulary: Vec<String>,
}

impl Summarize {
    fn system_prompt(&self) -> String {
        let mut prompt = format!(
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
        );
        if !self.vocabulary.is_empty() {
            prompt.push_str(&format!(
                "\nNames and terms that may come up, spelled correctly: {}. Use these spellings.",
                self.vocabulary.join(", ")
            ));
        }
        prompt
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Segment;

    fn seg(start_ms: u64, end_ms: u64, speaker: Option<&str>) -> Segment {
        Segment { start_ms, end_ms, text: "x".into(), speaker: speaker.map(Into::into) }
    }

    fn turn(start_ms: u64, end_ms: u64, speaker: &str) -> SpeakerTurn {
        SpeakerTurn { start_ms, end_ms, speaker: speaker.into() }
    }

    fn labels(transcript: &Transcript) -> Vec<Option<&str>> {
        transcript.segments.iter().map(|s| s.speaker.as_deref()).collect()
    }

    #[test]
    fn numbers_voices_in_order_of_appearance() {
        let mut transcript = Transcript {
            language: "en".into(),
            segments: vec![seg(0, 4000, None), seg(4000, 9000, None), seg(9000, 12000, None), seg(20000, 21000, None)],
        };
        // The second segment is mostly B; the last one overlaps nobody and goes to the nearest.
        let turns = [turn(0, 4500, "SPEAKER_01"), turn(4500, 9000, "SPEAKER_00"), turn(9000, 15000, "SPEAKER_01")];
        let next = assign_speakers(&mut transcript, None, &turns, 1);
        assert_eq!(next, 3);
        assert_eq!(
            labels(&transcript),
            [Some("Hablante 1"), Some("Hablante 2"), Some("Hablante 1"), Some("Hablante 1")]
        );
    }

    #[test]
    fn only_touches_its_track_and_keeps_a_lone_voice() {
        let mut transcript = Transcript {
            language: "es".into(),
            segments: vec![seg(0, 1000, Some("Yo")), seg(1000, 2000, Some("Ellos")), seg(2000, 3000, Some("Ellos"))],
        };
        let one_voice = [turn(1000, 3000, "SPEAKER_00")];
        assert_eq!(assign_speakers(&mut transcript, Some("Ellos"), &one_voice, 1), 1);
        assert_eq!(labels(&transcript), [Some("Yo"), Some("Ellos"), Some("Ellos")]);

        let two_voices = [turn(1000, 2000, "SPEAKER_00"), turn(2000, 3000, "SPEAKER_01")];
        assign_speakers(&mut transcript, Some("Ellos"), &two_voices, 1);
        assert_eq!(labels(&transcript), [Some("Yo"), Some("Hablante 1"), Some("Hablante 2")]);
    }

    #[test]
    fn renames_and_lists_speakers() {
        let mut transcript = Transcript {
            language: "es".into(),
            segments: vec![seg(0, 1, Some("Hablante 2")), seg(1, 2, Some("Yo")), seg(2, 3, Some("Hablante 2"))],
        };
        rename_speaker(&mut transcript, "Hablante 2", "María");
        assert_eq!(speakers(&transcript), ["María", "Yo"]);
    }
}
