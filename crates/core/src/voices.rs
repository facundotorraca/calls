//! Voice fingerprints (speaker embeddings): each call keeps one per voice, and the app
//! keeps one of the person using it, learnt from the voices they mark as theirs. With it,
//! they are recognized in the next calls.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A call's voices, by speaker label.
pub const VOICES_FILE: &str = "voices.json";

/// How alike two voices must be to be the same person (cosine similarity). Measured on a
/// real call split in two: the same person scored 0.91 to 0.99, different people 0.48 at
/// most.
pub const SAME_PERSON: f32 = 0.7;

pub type Voices = BTreeMap<String, Vec<f32>>;

pub fn load(dir: &Path) -> Voices {
    fs::read_to_string(dir.join(VOICES_FILE)).ok().and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
}

pub fn save(dir: &Path, voices: &Voices) -> Result<()> {
    fs::write(dir.join(VOICES_FILE), serde_json::to_vec(voices)?)?;
    Ok(())
}

/// Follows a speaker rename. Merging into a voice that is already there keeps that one.
pub fn rename(dir: &Path, from: &str, to: &str) -> Result<()> {
    let mut voices = load(dir);
    if let Some(voice) = voices.remove(from) {
        voices.entry(to.into()).or_insert(voice);
        save(dir, &voices)?;
    }
    Ok(())
}

/// The voice of the person using the app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyVoice {
    /// Mean of the (normalized) voices learnt.
    pub embedding: Vec<f32>,
    /// How many voices it was learnt from.
    pub samples: u32,
}

impl MyVoice {
    pub fn load(path: &Path) -> Option<MyVoice> {
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    /// Adds a voice marked as theirs.
    pub fn learn(path: &Path, voice: &[f32]) -> Result<MyVoice> {
        let voice = normalized(voice);
        let mine = match MyVoice::load(path).filter(|m| m.embedding.len() == voice.len()) {
            Some(MyVoice { embedding, samples }) => {
                let n = samples as f32;
                let embedding = embedding.iter().zip(&voice).map(|(m, v)| (m * n + v) / (n + 1.0)).collect();
                MyVoice { embedding, samples: samples + 1 }
            }
            None => MyVoice { embedding: voice, samples: 1 },
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, serde_json::to_vec(&mine)?)?;
        Ok(mine)
    }

    /// The label among `voices` that is this person, if one clearly is.
    pub fn find<'a>(&self, voices: impl IntoIterator<Item = (&'a String, &'a Vec<f32>)>) -> Option<&'a str> {
        voices
            .into_iter()
            .map(|(label, voice)| (label, similarity(&self.embedding, voice)))
            .filter(|(_, score)| *score >= SAME_PERSON)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(label, _)| label.as_str())
    }
}

fn normalized(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 { v.to_vec() } else { v.iter().map(|x| x / norm).collect() }
}

pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norms = a.iter().map(|x| x * x).sum::<f32>().sqrt() * b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norms == 0.0 { 0.0 } else { dot / norms }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_and_recognizes() {
        let dir = std::env::temp_dir().join(format!("calls-voices-{}", std::process::id()));
        let path = dir.join("voice.json");
        let _ = fs::remove_file(&path);

        MyVoice::learn(&path, &[1.0, 0.1, 0.0]).unwrap();
        let mine = MyVoice::learn(&path, &[2.0, 0.0, 0.2]).unwrap();
        assert_eq!(mine.samples, 2);

        let voices: Voices =
            [("Hablante 1".to_string(), vec![0.0, 1.0, 0.0]), ("Hablante 2".to_string(), vec![0.9, 0.1, 0.1])]
                .into_iter()
                .collect();
        assert_eq!(mine.find(&voices), Some("Hablante 2"));
        let strangers: Voices = [("Hablante 1".to_string(), vec![0.0, 1.0, 0.0])].into_iter().collect();
        assert_eq!(mine.find(&strangers), None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn follows_renames() {
        let dir = std::env::temp_dir().join(format!("calls-voices-rename-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        save(&dir, &[("Hablante 1".to_string(), vec![1.0]), ("Ana".to_string(), vec![2.0])].into_iter().collect())
            .unwrap();
        rename(&dir, "Hablante 1", "Juan").unwrap();
        rename(&dir, "Juan", "Ana").unwrap();
        assert_eq!(load(&dir), [("Ana".to_string(), vec![2.0])].into_iter().collect());
        fs::remove_dir_all(dir).unwrap();
    }
}
