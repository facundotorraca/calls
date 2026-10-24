use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local};

use crate::call::{CALL_FILE, TrackRole};
use crate::transcript::TRANSCRIPT_JSON;
use crate::{Call, CallId, Status, Track, Transcript};

/// The folder holding every call, one sub-folder per call:
///
/// ```text
/// ~/Calls/2026-10-23_1530_acme-kickoff/
///   call.json         metadata (source of truth)
///   audio.wav         extracted audio
///   transcript.json   segments with timestamps
///   transcript.md     human readable
/// ```
///
/// Plain files on purpose: easy to back up, grep or feed to other tools.
#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> Result<Library> {
        let root = root.into();
        fs::create_dir_all(&root).with_context(|| format!("creating library at {}", root.display()))?;
        Ok(Library { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn dir(&self, id: &CallId) -> PathBuf {
        self.root.join(&id.0)
    }

    /// All calls, newest first. Folders without a readable `call.json` are ignored.
    pub fn list(&self) -> Result<Vec<Call>> {
        let mut calls = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path().join(CALL_FILE);
            if let Ok(json) = fs::read_to_string(&path) {
                match serde_json::from_str::<Call>(&json) {
                    Ok(call) => calls.push(call),
                    Err(e) => eprintln!("skipping {}: {e}", path.display()),
                }
            }
        }
        calls.sort_by_key(|c| std::cmp::Reverse(c.created_at));
        Ok(calls)
    }

    pub fn load(&self, id: &CallId) -> Result<Call> {
        let path = self.dir(id).join(CALL_FILE);
        let json = fs::read_to_string(&path).with_context(|| format!("call {id} not found"))?;
        serde_json::from_str(&json).with_context(|| format!("invalid {}", path.display()))
    }

    pub fn save(&self, call: &Call) -> Result<()> {
        let dir = self.dir(&call.id);
        fs::create_dir_all(&dir)?;
        // Write then rename, so a crash never leaves a half written call.json behind.
        let tmp = dir.join(format!("{CALL_FILE}.tmp"));
        fs::write(&tmp, serde_json::to_vec_pretty(call)?)?;
        fs::rename(tmp, dir.join(CALL_FILE))?;
        Ok(())
    }

    /// Registers a recording as a new call. Importing the same file twice returns the existing call.
    pub fn import(&self, source: &Path) -> Result<Call> {
        let source = source.canonicalize().with_context(|| format!("{} not found", source.display()))?;
        if !source.is_file() {
            bail!("{} is not a file", source.display());
        }
        if let Some(existing) = self.list()?.into_iter().find(|c| c.source.as_deref() == Some(&source)) {
            return Ok(existing);
        }

        let title = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Llamada".into());
        // Creation time of the recording is when the call happened, not when it was imported.
        let meta = fs::metadata(&source)?;
        let created_at: DateTime<Local> =
            meta.created().or_else(|_| meta.modified()).map(Into::into).unwrap_or_else(|_| Local::now());

        let call = Call {
            id: self.unique_id(&format!("{}_{}", created_at.format("%Y-%m-%d_%H%M"), slug(&title))),
            title,
            created_at,
            source: Some(source),
            duration_ms: None,
            tracks: Vec::new(),
            status: Status::Pending,
            completed_steps: BTreeMap::new(),
        };
        self.save(&call)?;
        Ok(call)
    }

    /// Creates the call a native recording writes into: your mic and the other side as separate
    /// tracks. File names are part of the protocol with the audio sidecar.
    pub fn create_recording(&self, title: &str) -> Result<Call> {
        let created_at = Local::now();
        let call = Call {
            id: self.unique_id(&format!("{}_{}", created_at.format("%Y-%m-%d_%H%M"), slug(title))),
            title: title.into(),
            created_at,
            source: None,
            duration_ms: None,
            tracks: vec![
                Track { file: "mic.m4a".into(), speaker: Some("Yo".into()), role: TrackRole::Microphone, offset_ms: 0 },
                Track {
                    file: "system.m4a".into(),
                    speaker: Some("Ellos".into()),
                    role: TrackRole::System,
                    offset_ms: 0,
                },
            ],
            status: Status::Recording,
            completed_steps: BTreeMap::new(),
        };
        self.save(&call)?;
        Ok(call)
    }

    pub fn transcript(&self, id: &CallId) -> Result<Option<Transcript>> {
        let path = self.dir(id).join(TRANSCRIPT_JSON);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    fn unique_id(&self, base: &str) -> CallId {
        let mut id = base.to_string();
        let mut n = 2;
        while self.root.join(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        CallId(id)
    }
}

/// `"Reunión con ACME (v2)"` -> `"reunion-con-acme-v2"`.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(48).collect();
    if out.is_empty() { "llamada".into() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Reunión con ACME (v2)"), "reunion-con-acme-v2");
        assert_eq!(slug("2026-10-23 15-30-12"), "2026-10-23-15-30-12");
        assert_eq!(slug("???"), "llamada");
    }

    #[test]
    fn import_is_idempotent_and_ids_are_unique() {
        let tmp = std::env::temp_dir().join(format!("calls-test-{}", std::process::id()));
        let library = Library::open(tmp.join("lib")).unwrap();
        fs::write(tmp.join("Kickoff ACME.mkv"), b"x").unwrap();
        fs::write(tmp.join("kickoff-acme.mkv"), b"x").unwrap();

        let a = library.import(&tmp.join("Kickoff ACME.mkv")).unwrap();
        let again = library.import(&tmp.join("Kickoff ACME.mkv")).unwrap();
        let b = library.import(&tmp.join("kickoff-acme.mkv")).unwrap();

        assert_eq!(a.id, again.id);
        assert_ne!(a.id, b.id);
        assert!(a.id.0.ends_with("_kickoff-acme"));
        assert_eq!(library.list().unwrap().len(), 2);
        fs::remove_dir_all(tmp).unwrap();
    }
}
