//! Finding calls by what was said or written in them: the title, the summary, the notes and
//! every line of the transcript. Plain scanning of the files: a library is hundreds of calls,
//! not millions, and an index would be one more thing to keep in step with the files.

use anyhow::Result;
use serde::Serialize;

use crate::notes::Notes;
use crate::summary::Summary;
use crate::{Call, CallId, Library};

/// Passages returned per call; `SearchResult::total` says how many matched.
pub const MAX_HITS: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchResult {
    pub id: CallId,
    /// The best passages, the ones with the most of the query's words first.
    pub hits: Vec<Hit>,
    /// Passages with any of the words, beyond the ones in `hits`.
    pub total: usize,
}

/// A passage of a call that has words of the query.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hit {
    pub kind: HitKind,
    pub text: String,
    /// Where in the call, for transcript lines and marked moments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HitKind {
    Summary,
    Decision,
    Task,
    Notes,
    Mark,
    Transcript,
}

/// Lowercase without accents, one character for each character of `text`, so that a
/// position in the folded text is the same position in the original. The front end folds
/// the same way to highlight what matched.
pub fn fold(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

fn fold_char(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        _ => c,
    }
}

/// The query's words, folded, without repeats.
pub fn words(query: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for word in fold(query).split_whitespace() {
        if !words.iter().any(|w| w == word) {
            words.push(word.into());
        }
    }
    words
}

/// The calls that have every word of the query somewhere (title, folder, summary, notes or
/// transcript), those with all of them in the title first, then newest first.
pub fn search(library: &Library, query: &str) -> Result<Vec<SearchResult>> {
    let words = words(query);
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let mut titled = Vec::new();
    let mut rest = Vec::new();
    // `list` is newest first.
    for call in library.list()? {
        let dir = library.dir(&call.id);
        let transcript = library.transcript(&call.id).ok().flatten();
        let summary = Summary::load(&dir).ok().flatten();
        let notes = Notes::load(&dir).unwrap_or_default();
        let passages = passages(&summary, &notes, transcript.as_ref());
        if let Some(result) = match_call(&call, &passages, &words) {
            let title = fold(&call.title);
            if words.iter().all(|w| title.contains(w.as_str())) { titled.push(result) } else { rest.push(result) }
        }
    }
    titled.extend(rest);
    Ok(titled)
}

/// Everything in a call that can be found, as passages in reading order.
fn passages(summary: &Option<Summary>, notes: &Notes, transcript: Option<&crate::Transcript>) -> Vec<Hit> {
    let hit = |kind, text: &str| Hit { kind, text: text.trim().into(), at_ms: None, speaker: None };
    let mut out = Vec::new();
    if let Some(summary) = summary {
        out.extend(summary.summary.split('\n').filter(|p| !p.trim().is_empty()).map(|p| hit(HitKind::Summary, p)));
        out.extend(summary.decisions.iter().map(|d| hit(HitKind::Decision, d)));
        out.extend(summary.action_items.iter().map(|a| {
            let mut text = a.task.clone();
            if let Some(owner) = &a.owner {
                text = format!("{owner}: {text}");
            }
            if let Some(due) = &a.due {
                text.push_str(&format!(" ({due})"));
            }
            hit(HitKind::Task, &text)
        }));
    }
    out.extend(notes.text.split('\n').filter(|p| !p.trim().is_empty()).map(|p| hit(HitKind::Notes, p)));
    out.extend(notes.marks.iter().map(|m| Hit { at_ms: Some(m.at_ms), ..hit(HitKind::Mark, &m.text) }));
    if let Some(transcript) = transcript {
        out.extend(transcript.segments.iter().map(|s| Hit {
            kind: HitKind::Transcript,
            text: s.text.trim().into(),
            at_ms: Some(s.start_ms),
            speaker: s.speaker.clone(),
        }));
    }
    out
}

fn match_call(call: &Call, passages: &[Hit], words: &[String]) -> Option<SearchResult> {
    let folded: Vec<String> = passages.iter().map(|p| fold(&p.text)).collect();
    let head = fold(&format!("{} {}", call.title, call.folder.as_deref().unwrap_or("")));
    let found = |word: &String| head.contains(word.as_str()) || folded.iter().any(|t| t.contains(word.as_str()));
    if !words.iter().all(found) {
        return None;
    }

    // How many of the words each passage has; ties keep reading order (the sort is stable).
    let mut scored: Vec<(usize, &Hit)> = passages
        .iter()
        .zip(&folded)
        .map(|(p, t)| (words.iter().filter(|w| t.contains(w.as_str())).count(), p))
        .filter(|(n, _)| *n > 0)
        .collect();
    scored.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    let total = scored.len();
    let hits = scored.into_iter().take(MAX_HITS).map(|(_, p)| p.clone()).collect();
    Some(SearchResult { id: call.id.clone(), hits, total })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::Mark;
    use crate::{Segment, Transcript};

    fn call(title: &str) -> Call {
        serde_json::from_value(serde_json::json!({
            "id": "c", "title": title, "created_at": "2026-10-23T15:30:00-03:00",
            "source": null, "duration_ms": null, "tracks": [], "status": { "state": "ready" }
        }))
        .unwrap()
    }

    fn segment(start_ms: u64, text: &str) -> Segment {
        Segment { start_ms, end_ms: start_ms + 1000, text: text.into(), speaker: Some("Martina".into()) }
    }

    #[test]
    fn folding_keeps_positions() {
        assert_eq!(fold("Reunión CON Ñandú"), "reunion con nandu");
        assert_eq!(fold("Reunión").chars().count(), "Reunión".chars().count());
        assert_eq!(words("  precio  Precio plazo "), ["precio", "plazo"]);
    }

    #[test]
    fn every_word_somewhere_and_best_passages_first() {
        let transcript = Transcript {
            language: "es".into(),
            segments: vec![segment(1000, "Hablemos del precio."), segment(5000, "El precio y el plazo de entrega.")],
        };
        let notes = Notes { text: String::new(), marks: vec![Mark { at_ms: 7000, text: "plazo".into() }] };
        let passages = passages(&None, &notes, Some(&transcript));

        let result = match_call(&call("Kickoff"), &passages, &words("plazo precio")).unwrap();
        assert_eq!(result.total, 3);
        assert_eq!(result.hits[0].at_ms, Some(5000), "the passage with both words comes first");
        assert_eq!(result.hits[1].kind, HitKind::Mark, "then what they wrote, before the transcript");
        assert_eq!(result.hits[2].at_ms, Some(1000));

        assert!(match_call(&call("Kickoff"), &passages, &words("precio contrato")).is_none());
        // A word in the title counts, without being a passage.
        let result = match_call(&call("Contrato Acme"), &passages, &words("precio contrato")).unwrap();
        assert_eq!(result.total, 2);
    }
}
