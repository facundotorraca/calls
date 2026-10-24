use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::call::{CALL_FILE, TrackRole};
use crate::summary::{SUMMARY_MD, Summary};
use crate::transcript::{TRANSCRIPT_JSON, TRANSCRIPT_MD};
use crate::{Call, CallId, Status, Track, Transcript};

/// Library wide data that does not belong to a call, stored at the library root.
const INDEX_FILE: &str = "library.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    /// Folder names, kept here so empty folders survive.
    #[serde(default)]
    folders: Vec<String>,
}

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
///
/// Clones share a lock, so edits from the UI and the pipeline saving progress never
/// overwrite each other.
#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> Result<Library> {
        let root = root.into();
        fs::create_dir_all(&root).with_context(|| format!("creating library at {}", root.display()))?;
        Ok(Library { root, lock: Arc::default() })
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
        let _guard = self.lock();
        self.write(call)
    }

    /// Loads, changes and saves a call as one operation.
    pub fn update(&self, id: &CallId, change: impl FnOnce(&mut Call)) -> Result<Call> {
        let _guard = self.lock();
        let mut call = self.load(id)?;
        change(&mut call);
        self.write(&call)?;
        Ok(call)
    }

    /// Saves processing state. The title and folder on disk win over `call`'s, since the
    /// user may have edited them while the call was being processed.
    pub fn save_progress(&self, call: &mut Call) -> Result<()> {
        let _guard = self.lock();
        if let Ok(saved) = self.load(&call.id) {
            call.title = saved.title;
            call.folder = saved.folder;
        }
        self.write(call)
    }

    /// Renames a call, and its documents, which carry the title.
    pub fn rename(&self, id: &CallId, title: &str) -> Result<Call> {
        let title = title.trim();
        if title.is_empty() {
            bail!("the title can not be empty");
        }
        let call = self.update(id, |call| call.title = title.into())?;
        let dir = self.dir(id);
        if let Some(transcript) = self.transcript(id)? {
            fs::write(dir.join(TRANSCRIPT_MD), transcript.to_markdown(title))?;
        }
        if let Some(summary) = Summary::load(&dir)? {
            fs::write(dir.join(SUMMARY_MD), summary.to_markdown(title))?;
        }
        Ok(call)
    }

    /// Moves a call into a folder (created if needed), or out of any with `None`.
    pub fn move_to(&self, id: &CallId, folder: Option<&str>) -> Result<Call> {
        let folder = match folder {
            Some(name) => Some(self.create_folder(name)?),
            None => None,
        };
        self.update(id, |call| call.folder = folder)
    }

    /// Folder names, alphabetically.
    pub fn folders(&self) -> Result<Vec<String>> {
        Ok(self.index()?.folders)
    }

    /// Creates a folder and returns its name. A folder with the same name, ignoring case,
    /// is reused.
    pub fn create_folder(&self, name: &str) -> Result<String> {
        let name = folder_name(name)?;
        let _guard = self.lock();
        let mut index = self.index()?;
        if let Some(existing) = index.folders.iter().find(|f| f.to_lowercase() == name.to_lowercase()) {
            return Ok(existing.clone());
        }
        index.folders.push(name.clone());
        self.write_index(&mut index)?;
        Ok(name)
    }

    /// Renames a folder, moving its calls along.
    pub fn rename_folder(&self, from: &str, to: &str) -> Result<()> {
        let to = folder_name(to)?;
        let _guard = self.lock();
        let mut index = self.index()?;
        if from != to && index.folders.iter().any(|f| f.to_lowercase() == to.to_lowercase() && f != from) {
            bail!("there is already a folder named {to}");
        }
        let Some(folder) = index.folders.iter_mut().find(|f| *f == from) else { bail!("folder {from} not found") };
        *folder = to.clone();
        self.write_index(&mut index)?;
        self.relabel(from, Some(&to))
    }

    /// Deletes a folder. Its calls stay in the library, outside any folder.
    pub fn delete_folder(&self, name: &str) -> Result<()> {
        let _guard = self.lock();
        let mut index = self.index()?;
        index.folders.retain(|f| f != name);
        self.write_index(&mut index)?;
        self.relabel(name, None)
    }

    /// Changes the folder of every call in `from`. Callers hold the lock.
    fn relabel(&self, from: &str, to: Option<&str>) -> Result<()> {
        for mut call in self.list()? {
            if call.folder.as_deref() == Some(from) {
                call.folder = to.map(Into::into);
                self.write(&call)?;
            }
        }
        Ok(())
    }

    fn index(&self) -> Result<Index> {
        let path = self.root.join(INDEX_FILE);
        if !path.exists() {
            return Ok(Index::default());
        }
        serde_json::from_str(&fs::read_to_string(&path)?).with_context(|| format!("invalid {}", path.display()))
    }

    fn write_index(&self, index: &mut Index) -> Result<()> {
        index.folders.sort_by_key(|f| f.to_lowercase());
        write_atomic(&self.root.join(INDEX_FILE), &serde_json::to_vec_pretty(index)?)
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write(&self, call: &Call) -> Result<()> {
        let dir = self.dir(&call.id);
        fs::create_dir_all(&dir)?;
        write_atomic(&dir.join(CALL_FILE), &serde_json::to_vec_pretty(call)?)
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
            folder: None,
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
            folder: None,
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

/// Writes then renames, so a crash never leaves a half written file behind.
fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn folder_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("the folder name can not be empty");
    }
    Ok(name.into())
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

    #[test]
    fn folders_and_user_edits() {
        let tmp = std::env::temp_dir().join(format!("calls-test-folders-{}", std::process::id()));
        let library = Library::open(tmp.join("lib")).unwrap();
        fs::write(tmp.join("call.mkv"), b"x").unwrap();
        let mut processing = library.import(&tmp.join("call.mkv")).unwrap();

        library.move_to(&processing.id, Some(" Clientes ")).unwrap();
        library.rename(&processing.id, "Kickoff ACME").unwrap();
        assert_eq!(library.create_folder("clientes").unwrap(), "Clientes");
        library.create_folder("Acme").unwrap();
        assert_eq!(library.folders().unwrap(), ["Acme", "Clientes"]);

        // The pipeline saving its copy keeps what the user changed meanwhile.
        processing.status = Status::Ready;
        library.save_progress(&mut processing).unwrap();
        let call = library.load(&processing.id).unwrap();
        assert_eq!(
            (call.title.as_str(), call.folder.as_deref(), call.status),
            ("Kickoff ACME", Some("Clientes"), Status::Ready)
        );

        library.rename_folder("Clientes", "Clientes 2026").unwrap();
        assert_eq!(library.load(&call.id).unwrap().folder.as_deref(), Some("Clientes 2026"));
        assert!(library.rename_folder("Acme", "clientes 2026").is_err());

        library.delete_folder("Clientes 2026").unwrap();
        assert_eq!(library.load(&call.id).unwrap().folder, None);
        assert_eq!(library.folders().unwrap(), ["Acme"]);
        fs::remove_dir_all(tmp).unwrap();
    }
}
