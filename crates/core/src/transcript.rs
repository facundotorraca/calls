use serde::{Deserialize, Serialize};

pub const TRANSCRIPT_JSON: &str = "transcript.json";
pub const TRANSCRIPT_MD: &str = "transcript.md";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    /// Language detected (or forced) by the transcriber, e.g. `es`.
    pub language: String,
    pub segments: Vec<Segment>,
}

impl Transcript {
    /// Interleaves the transcripts of several tracks into a single conversation.
    pub fn merge(parts: Vec<Transcript>) -> Transcript {
        let language = parts.first().map(|t| t.language.clone()).unwrap_or_default();
        let mut segments: Vec<Segment> = parts.into_iter().flat_map(|t| t.segments).collect();
        segments.sort_by_key(|s| (s.start_ms, s.end_ms));
        Transcript { language, segments }
    }

    pub fn to_markdown(&self, title: &str) -> String {
        let mut md = format!("# {title}\n\n");
        for segment in &self.segments {
            md.push_str(&format!("**[{}]**", format_timestamp(segment.start_ms)));
            if let Some(speaker) = &segment.speaker {
                md.push_str(&format!(" **{speaker}:**"));
            }
            md.push_str(&format!(" {}\n\n", segment.text));
        }
        md
    }
}

/// Without headphones the other side leaks into your microphone, so the same sentence shows
/// up in both tracks. Drops the microphone segments that repeat a system segment: same time,
/// mostly the same words.
pub fn remove_echo(mic: &mut Transcript, system: &Transcript) {
    mic.segments.retain(|m| {
        !system.segments.iter().any(|s| overlap_ratio(m, s) >= 0.5 && word_similarity(&m.text, &s.text) >= 0.5)
    });
}

/// Fraction of `a`'s duration that overlaps `b`.
fn overlap_ratio(a: &Segment, b: &Segment) -> f32 {
    let overlap = a.end_ms.min(b.end_ms).saturating_sub(a.start_ms.max(b.start_ms));
    overlap as f32 / a.end_ms.saturating_sub(a.start_ms).max(1) as f32
}

/// Jaccard similarity of the sets of words.
fn word_similarity(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let words = |s: &str| -> HashSet<String> {
        s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
    };
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(&b).count() as f32 / a.union(&b).count() as f32
}

/// `mm:ss`, or `h:mm:ss` once past the hour.
pub fn format_timestamp(ms: u64) -> String {
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start_ms: u64, speaker: &str) -> Segment {
        Segment { start_ms, end_ms: start_ms + 1000, text: format!("at {start_ms}"), speaker: Some(speaker.into()) }
    }

    #[test]
    fn merge_interleaves_tracks_by_time() {
        let me = Transcript { language: "es".into(), segments: vec![seg(0, "Yo"), seg(5000, "Yo")] };
        let them = Transcript { language: "es".into(), segments: vec![seg(2000, "Cliente")] };

        let merged = Transcript::merge(vec![me, them]);

        let order: Vec<_> = merged.segments.iter().map(|s| s.start_ms).collect();
        assert_eq!(order, [0, 2000, 5000]);
        assert_eq!(merged.language, "es");
    }

    #[test]
    fn echo_of_the_other_side_is_removed_from_the_mic() {
        let segment =
            |start_ms, text: &str| Segment { start_ms, end_ms: start_ms + 3000, text: text.into(), speaker: None };
        let system = Transcript {
            language: "en".into(),
            segments: vec![segment(10_000, "We need the validation from your end so we can close it.")],
        };
        let mut mic = Transcript {
            language: "en".into(),
            segments: vec![
                segment(10_200, "we need the validation from your end so we can close"),
                segment(14_000, "Sure, I'll send it on Monday."),
            ],
        };

        remove_echo(&mut mic, &system);

        assert_eq!(mic.segments.len(), 1);
        assert_eq!(mic.segments[0].text, "Sure, I'll send it on Monday.");
    }

    #[test]
    fn timestamps() {
        assert_eq!(format_timestamp(65_400), "01:05");
        assert_eq!(format_timestamp(3_725_000), "1:02:05");
    }
}
