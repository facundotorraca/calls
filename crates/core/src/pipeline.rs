use std::path::Path;

use anyhow::Result;
use chrono::Local;
use serde::Serialize;

use crate::{Call, CallId, Library, Status};

/// One unit of processing applied to a call (extract audio, transcribe, summarize, export...).
///
/// Adding a feature = writing a `Step` and adding it to the pipeline. Steps must be
/// idempotent: if one fails, the whole step runs again on the next attempt.
pub trait Step: Send + Sync {
    /// Stable identifier, persisted in `call.json`. Never rename it once shipped.
    fn id(&self) -> &'static str;

    fn run(&self, call: &mut Call, ctx: &StepContext) -> Result<()>;
}

pub struct StepContext<'a> {
    /// The call's folder, where steps read and write their files.
    pub dir: &'a Path,
    progress: &'a dyn Fn(f32),
}

impl StepContext<'_> {
    pub fn progress(&self, fraction: f32) {
        (self.progress)(fraction.clamp(0.0, 1.0));
    }
}

/// What the pipeline reports while it runs, so front ends can show progress.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    StepStarted { call_id: CallId, step: String },
    StepProgress { call_id: CallId, step: String, fraction: f32 },
    StepFinished { call_id: CallId, step: String },
    Finished { call_id: CallId },
    Failed { call_id: CallId, step: String, error: String },
}

#[derive(Default)]
pub struct Pipeline {
    steps: Vec<Box<dyn Step>>,
}

impl Pipeline {
    pub fn new() -> Pipeline {
        Pipeline::default()
    }

    pub fn with(mut self, step: impl Step + 'static) -> Pipeline {
        self.steps.push(Box::new(step));
        self
    }

    pub fn step_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.steps.iter().map(|s| s.id())
    }

    /// Runs every step not yet completed for this call, saving state after each one.
    pub fn run(&self, library: &Library, id: &CallId, emit: &dyn Fn(Event)) -> Result<()> {
        let mut call = library.load(id)?;
        let dir = library.dir(id);

        for step in &self.steps {
            let step_id = step.id();
            if call.completed_steps.contains_key(step_id) {
                continue;
            }

            call.status = Status::Processing { step: step_id.into() };
            library.save_progress(&mut call)?;
            emit(Event::StepStarted { call_id: id.clone(), step: step_id.into() });

            let progress = |fraction| emit(Event::StepProgress { call_id: id.clone(), step: step_id.into(), fraction });
            let ctx = StepContext { dir: &dir, progress: &progress };

            if let Err(e) = step.run(&mut call, &ctx) {
                let error = format!("{e:#}");
                call.status = Status::Failed { step: step_id.into(), error: error.clone() };
                library.save_progress(&mut call)?;
                emit(Event::Failed { call_id: id.clone(), step: step_id.into(), error });
                return Err(e);
            }

            call.completed_steps.insert(step_id.into(), Local::now());
            library.save_progress(&mut call)?;
            emit(Event::StepFinished { call_id: id.clone(), step: step_id.into() });
        }

        call.status = Status::Ready;
        library.save_progress(&mut call)?;
        emit(Event::Finished { call_id: id.clone() });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use anyhow::bail;

    use super::*;

    struct Flaky {
        runs: AtomicUsize,
        fail_first: bool,
    }

    impl Step for Flaky {
        fn id(&self) -> &'static str {
            "flaky"
        }

        fn run(&self, _: &mut Call, ctx: &StepContext) -> Result<()> {
            ctx.progress(0.5);
            if self.runs.fetch_add(1, Ordering::SeqCst) == 0 && self.fail_first {
                bail!("boom");
            }
            Ok(())
        }
    }

    #[test]
    fn failed_steps_are_retried_and_completed_ones_skipped() {
        let tmp = std::env::temp_dir().join(format!("calls-pipeline-{}", std::process::id()));
        let library = Library::open(tmp.join("lib")).unwrap();
        fs::write(tmp.join("call.mkv"), b"x").unwrap();
        let call = library.import(&tmp.join("call.mkv")).unwrap();

        let pipeline = Pipeline::new().with(Flaky { runs: AtomicUsize::new(0), fail_first: true });
        let events = Mutex::new(Vec::new());
        let emit = |e: Event| events.lock().unwrap().push(e);

        assert!(pipeline.run(&library, &call.id, &emit).is_err());
        assert!(matches!(library.load(&call.id).unwrap().status, Status::Failed { .. }));

        pipeline.run(&library, &call.id, &emit).unwrap();
        assert_eq!(library.load(&call.id).unwrap().status, Status::Ready);

        // Already completed: a third run does not execute the step again.
        events.lock().unwrap().clear();
        pipeline.run(&library, &call.id, &emit).unwrap();
        assert!(matches!(events.lock().unwrap().as_slice(), [Event::Finished { .. }]));

        fs::remove_dir_all(tmp).unwrap();
    }
}
