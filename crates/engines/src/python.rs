//! Runs Python workers (`workers/<name>`), sidecars for what only exists in Python.
//! See workers/README.md for the protocol.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use calls_core::engines::Progress;
use calls_core::settings;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::process::{Stream, run_lines};
use crate::sidecar;

pub struct PythonWorker {
    name: String,
    uv: PathBuf,
    project: PathBuf,
}

impl PythonWorker {
    pub fn locate(name: &str) -> Result<PythonWorker> {
        let uv = sidecar::locate("uv").context("uv not found (brew install uv)")?;
        let project = workers_dir()
            .map(|dir| dir.join(name))
            .filter(|dir| dir.join("main.py").is_file())
            .with_context(|| format!("worker {name} not found"))?;
        Ok(PythonWorker { name: name.into(), uv, project })
    }

    /// Sends `request` and returns the worker's result. The first run also creates the
    /// worker's environment, which can take a while if it has heavy dependencies.
    pub fn run<T: DeserializeOwned>(&self, request: &impl Serialize, progress: Progress) -> Result<T> {
        let mut cmd = Command::new(&self.uv);
        cmd.args(["run", "--frozen", "--quiet", "--project"])
            .arg(&self.project)
            .arg("python")
            .arg(self.project.join("main.py"))
            // Outside the project: the app bundle is read only (and signed).
            .env("UV_PROJECT_ENVIRONMENT", settings::data_dir().join("workers").join(&self.name));

        let mut result = None;
        let mut error = None;
        run_lines(cmd, Some(serde_json::to_vec(request)?), Stream::Stdout, |line| {
            match serde_json::from_str::<Value>(line) {
                Ok(Value::Object(mut message)) => {
                    if let Some(fraction) = message.get("progress").and_then(Value::as_f64) {
                        progress(fraction as f32);
                    } else if let Some(value) = message.remove("result") {
                        result = Some(value);
                    } else if let Some(e) = message.get("error").and_then(Value::as_str) {
                        error = Some(e.to_string());
                    }
                }
                _ => eprintln!("worker {}: {line}", self.name),
            }
        })
        .map_err(|e| match &error {
            Some(message) => anyhow!("worker {}: {message}", self.name),
            None => e,
        })?;

        match (result, error) {
            (Some(value), _) => {
                serde_json::from_value(value).with_context(|| format!("unexpected result from worker {}", self.name))
            }
            (None, Some(message)) => bail!("worker {}: {message}", self.name),
            (None, None) => bail!("worker {} returned nothing", self.name),
        }
    }
}

/// `$CALLS_WORKERS_DIR`, the app bundle's resources, or the repo's `workers/` in debug builds.
fn workers_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = env::var_os("CALLS_WORKERS_DIR").map(PathBuf::from).into_iter().collect();
    if let Some(macos) = env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        candidates.push(macos.join("../Resources/workers"));
    }
    if cfg!(debug_assertions) {
        candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../workers"));
    }
    candidates.into_iter().find(|dir| dir.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_the_template_worker() {
        if sidecar::locate("uv").is_none() {
            eprintln!("skipped: uv not installed");
            return;
        }
        let worker = PythonWorker::locate("template").unwrap();
        let progress = std::sync::Mutex::new(Vec::new());

        let result: Value =
            worker.run(&serde_json::json!({ "hello": "world" }), &|f| progress.lock().unwrap().push(f)).unwrap();

        assert_eq!(result, serde_json::json!({ "echo": { "hello": "world" } }));
        assert_eq!(*progress.lock().unwrap(), [1.0]);
    }
}
