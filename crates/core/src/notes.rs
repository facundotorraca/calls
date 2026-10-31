//! What the person using the app wrote about a call: free notes, and moments marked while it
//! was happening (or later, while listening). They are theirs: processing never touches them,
//! and the summary reads them to know what mattered.

use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::transcript::format_timestamp;

pub const NOTES_JSON: &str = "notes.json";
pub const NOTES_MD: &str = "notes.md";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Notes {
    #[serde(default)]
    pub text: String,
    /// In order of time.
    #[serde(default)]
    pub marks: Vec<Mark>,
}

/// A moment of the call worth coming back to, with an optional line about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    /// From the start of the call.
    pub at_ms: u64,
    #[serde(default)]
    pub text: String,
}

impl Notes {
    /// Empty if the call has none.
    pub fn load(dir: &Path) -> Result<Notes> {
        let path = dir.join(NOTES_JSON);
        if !path.exists() {
            return Ok(Notes::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    /// Next to the JSON, a Markdown copy to read in the Finder. Empty notes leave no files.
    pub fn save(&mut self, dir: &Path, title: &str) -> Result<()> {
        self.marks.sort_by_key(|m| m.at_ms);
        if self.is_empty() {
            for file in [NOTES_JSON, NOTES_MD] {
                if dir.join(file).exists() {
                    fs::remove_file(dir.join(file))?;
                }
            }
            return Ok(());
        }
        fs::write(dir.join(NOTES_JSON), serde_json::to_vec_pretty(self)?)?;
        fs::write(dir.join(NOTES_MD), self.to_markdown(title))?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.marks.is_empty()
    }

    pub fn to_markdown(&self, title: &str) -> String {
        let mut md = format!("# {title}\n");
        if !self.text.trim().is_empty() {
            md.push_str(&format!("\n{}\n", self.text.trim()));
        }
        if !self.marks.is_empty() {
            md.push_str("\n## Momentos\n\n");
            for line in self.mark_lines() {
                md.push_str(&format!("- {line}\n"));
            }
        }
        md
    }

    /// For a model: the notes, then the marked moments, one per line. `None` if empty.
    pub fn to_prompt(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut out = String::new();
        if !self.text.trim().is_empty() {
            out.push_str(self.text.trim());
            out.push('\n');
        }
        if !self.marks.is_empty() {
            out.push_str("Moments they marked as important:\n");
            for line in self.mark_lines() {
                out.push_str(&format!("{line}\n"));
            }
        }
        Some(out)
    }

    fn mark_lines(&self) -> impl Iterator<Item = String> + '_ {
        self.marks.iter().map(|m| {
            let at = format_timestamp(m.at_ms);
            if m.text.trim().is_empty() { format!("[{at}]") } else { format!("[{at}] {}", m.text.trim()) }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_notes_leave_no_files() {
        let dir = std::env::temp_dir().join(format!("calls-notes-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut notes = Notes {
            text: "Preguntar por el precio".into(),
            marks: vec![Mark { at_ms: 90_000, text: "acá".into() }, Mark { at_ms: 5_000, text: String::new() }],
        };
        notes.save(&dir, "Kickoff").unwrap();
        let loaded = Notes::load(&dir).unwrap();
        assert_eq!(loaded.marks[0].at_ms, 5_000, "marks are kept in order of time");
        assert!(fs::read_to_string(dir.join(NOTES_MD)).unwrap().contains("- [01:30] acá"));

        notes = Notes::default();
        notes.save(&dir, "Kickoff").unwrap();
        assert!(!dir.join(NOTES_JSON).exists() && !dir.join(NOTES_MD).exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prompt_lists_the_moments() {
        let notes =
            Notes { text: " ojo con el plazo ".into(), marks: vec![Mark { at_ms: 65_000, text: String::new() }] };
        assert_eq!(notes.to_prompt().unwrap(), "ojo con el plazo\nMoments they marked as important:\n[01:05]\n");
        assert_eq!(Notes::default().to_prompt(), None);
    }
}
