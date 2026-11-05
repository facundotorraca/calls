use std::fmt;
use std::path::Path;

use anyhow::{Result, bail};
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

    /// A step the call can do without: when it fails, the call is still ready, and the error
    /// stays on the step (`Call::step_errors`) until it runs again.
    fn optional(&self) -> bool {
        false
    }

    /// Whether what it works on is there. Until it is, the step waits without a word: nothing
    /// went wrong, there is just nothing for it to do yet.
    fn ready(&self, _call: &Call, _dir: &Path) -> bool {
        true
    }

    fn run(&self, call: &mut Call, ctx: &StepContext) -> Result<()>;
}

pub struct StepContext<'a> {
    /// The call's folder, where steps read and write their files.
    pub dir: &'a Path,
    /// The user asked for this run (rather than a call that just ended or was imported):
    /// heavy steps run even if the computer is busy.
    pub on_demand: bool,
    progress: &'a dyn Fn(f32),
}

impl StepContext<'_> {
    pub fn progress(&self, fraction: f32) {
        (self.progress)(fraction.clamp(0.0, 1.0));
    }
}

/// What a step fails with to be left for later: it is not done, but the call goes on
/// without it, and runs it when processed again on demand.
#[derive(Debug)]
pub struct Postponed(pub &'static str);

impl fmt::Display for Postponed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "postponed: {}", self.0)
    }
}

impl std::error::Error for Postponed {}

/// What the pipeline reports while it runs, so front ends can show progress.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    StepStarted {
        call_id: CallId,
        step: String,
    },
    StepProgress {
        call_id: CallId,
        step: String,
        fraction: f32,
    },
    /// Left for later (see [`Postponed`]); the call is processed without it.
    StepPostponed {
        call_id: CallId,
        step: String,
    },
    StepFinished {
        call_id: CallId,
        step: String,
    },
    /// An optional step failed; the call goes on without it.
    StepFailed {
        call_id: CallId,
        step: String,
        error: String,
    },
    Finished {
        call_id: CallId,
    },
    Failed {
        call_id: CallId,
        step: String,
        error: String,
    },
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
    pub fn run(&self, library: &Library, id: &CallId, on_demand: bool, emit: &dyn Fn(Event)) -> Result<()> {
        let mut call = library.load(id)?;
        let dir = library.dir(id);

        for step in &self.steps {
            let step_id = step.id();
            if call.completed_steps.contains_key(step_id) || !step.ready(&call, &dir) {
                continue;
            }

            call.status = Status::Processing { step: step_id.into() };
            call.step_errors.remove(step_id);
            library.save_progress(&mut call)?;
            emit(Event::StepStarted { call_id: id.clone(), step: step_id.into() });

            let progress = |fraction| emit(Event::StepProgress { call_id: id.clone(), step: step_id.into(), fraction });
            let ctx = StepContext { dir: &dir, on_demand, progress: &progress };

            if let Err(e) = step.run(&mut call, &ctx) {
                if e.is::<Postponed>() {
                    emit(Event::StepPostponed { call_id: id.clone(), step: step_id.into() });
                    continue;
                }
                let error = format!("{e:#}");
                if step.optional() {
                    call.step_errors.insert(step_id.into(), error.clone());
                    library.save_progress(&mut call)?;
                    emit(Event::StepFailed { call_id: id.clone(), step: step_id.into(), error });
                    continue;
                }
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

    /// Forgets `from` and every step after it, for the next run to do them again. What they
    /// wrote stays until they replace it, and steps turned off keep their results.
    pub fn reset_from(&self, library: &Library, id: &CallId, from: &str) -> Result<()> {
        let Some(start) = self.steps.iter().position(|s| s.id() == from) else {
            bail!("{from} is not in the pipeline");
        };
        if matches!(library.load(id)?.status, Status::Recording | Status::Pending | Status::Processing { .. }) {
            bail!("the call is busy: wait for it to finish");
        }
        library.update(id, |call| {
            for step in &self.steps[start..] {
                call.completed_steps.remove(step.id());
                call.step_errors.remove(step.id());
            }
            call.status = Status::Pending;
        })?;
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

        assert!(pipeline.run(&library, &call.id, false, &emit).is_err());
        assert!(matches!(library.load(&call.id).unwrap().status, Status::Failed { .. }));

        pipeline.run(&library, &call.id, false, &emit).unwrap();
        assert_eq!(library.load(&call.id).unwrap().status, Status::Ready);

        // Already completed: a third run does not execute the step again.
        events.lock().unwrap().clear();
        pipeline.run(&library, &call.id, false, &emit).unwrap();
        assert!(matches!(events.lock().unwrap().as_slice(), [Event::Finished { .. }]));

        fs::remove_dir_all(tmp).unwrap();
    }

    /// Runs only when asked for.
    struct Heavy;

    impl Step for Heavy {
        fn id(&self) -> &'static str {
            "heavy"
        }

        fn run(&self, _: &mut Call, ctx: &StepContext) -> Result<()> {
            if !ctx.on_demand {
                return Err(Postponed("busy").into());
            }
            Ok(())
        }
    }

    #[test]
    fn postponed_steps_wait_for_the_user() {
        let tmp = std::env::temp_dir().join(format!("calls-pipeline-postponed-{}", std::process::id()));
        let library = Library::open(tmp.join("lib")).unwrap();
        fs::write(tmp.join("call.mkv"), b"x").unwrap();
        let call = library.import(&tmp.join("call.mkv")).unwrap();
        let pipeline = Pipeline::new().with(Heavy).with(Flaky { runs: AtomicUsize::new(0), fail_first: false });

        // The call is ready without it, and the steps after it still ran.
        pipeline.run(&library, &call.id, false, &|_| {}).unwrap();
        let processed = library.load(&call.id).unwrap();
        assert_eq!(processed.status, Status::Ready);
        assert!(!processed.completed_steps.contains_key("heavy"));
        assert!(processed.completed_steps.contains_key("flaky"));

        pipeline.run(&library, &call.id, true, &|_| {}).unwrap();
        assert!(library.load(&call.id).unwrap().completed_steps.contains_key("heavy"));
        fs::remove_dir_all(tmp).unwrap();
    }

    /// Fails the first time, and the call can do without it.
    struct Extra {
        runs: AtomicUsize,
    }

    impl Step for Extra {
        fn id(&self) -> &'static str {
            "extra"
        }

        fn optional(&self) -> bool {
            true
        }

        fn run(&self, _: &mut Call, _: &StepContext) -> Result<()> {
            if self.runs.fetch_add(1, Ordering::SeqCst) == 0 {
                bail!("no model");
            }
            Ok(())
        }
    }

    #[test]
    fn a_failed_optional_step_leaves_the_call_ready_with_its_error() {
        let tmp = std::env::temp_dir().join(format!("calls-pipeline-optional-{}", std::process::id()));
        let library = Library::open(tmp.join("lib")).unwrap();
        fs::write(tmp.join("call.mkv"), b"x").unwrap();
        let call = library.import(&tmp.join("call.mkv")).unwrap();
        let pipeline = Pipeline::new()
            .with(Extra { runs: AtomicUsize::new(0) })
            .with(Flaky { runs: AtomicUsize::new(0), fail_first: false });

        pipeline.run(&library, &call.id, false, &|_| {}).unwrap();
        let processed = library.load(&call.id).unwrap();
        assert_eq!(processed.status, Status::Ready);
        assert_eq!(processed.step_errors["extra"], "no model");
        assert!(processed.completed_steps.contains_key("flaky"));

        // Run again, it is retried and its error goes away.
        pipeline.run(&library, &call.id, true, &|_| {}).unwrap();
        let retried = library.load(&call.id).unwrap();
        assert!(retried.step_errors.is_empty());
        assert!(retried.completed_steps.contains_key("extra"));
        fs::remove_dir_all(tmp).unwrap();
    }

    /// Waits for another step to be done.
    struct After(&'static str);

    impl Step for After {
        fn id(&self) -> &'static str {
            "after"
        }

        fn ready(&self, call: &Call, _: &Path) -> bool {
            call.completed_steps.contains_key(self.0)
        }

        fn run(&self, _: &mut Call, _: &StepContext) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_step_not_ready_waits_without_a_word() {
        let tmp = std::env::temp_dir().join(format!("calls-pipeline-ready-{}", std::process::id()));
        let library = Library::open(tmp.join("lib")).unwrap();
        fs::write(tmp.join("call.mkv"), b"x").unwrap();
        let call = library.import(&tmp.join("call.mkv")).unwrap();
        let events = Mutex::new(Vec::new());
        let emit = |e: Event| events.lock().unwrap().push(e);

        Pipeline::new().with(After("heavy")).run(&library, &call.id, false, &emit).unwrap();
        assert!(matches!(events.lock().unwrap().as_slice(), [Event::Finished { .. }]));
        let waited = library.load(&call.id).unwrap();
        assert_eq!(waited.status, Status::Ready);
        assert!(!waited.completed_steps.contains_key("after"));

        Pipeline::new().with(Heavy).with(After("heavy")).run(&library, &call.id, true, &|_| {}).unwrap();
        assert!(library.load(&call.id).unwrap().completed_steps.contains_key("after"));
        fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn resetting_runs_a_step_and_the_ones_after_it_again() {
        let tmp = std::env::temp_dir().join(format!("calls-pipeline-reset-{}", std::process::id()));
        let library = Library::open(tmp.join("lib")).unwrap();
        fs::write(tmp.join("call.mkv"), b"x").unwrap();
        let call = library.import(&tmp.join("call.mkv")).unwrap();
        let pipeline = Pipeline::new().with(Heavy).with(Flaky { runs: AtomicUsize::new(0), fail_first: false });
        pipeline.run(&library, &call.id, true, &|_| {}).unwrap();

        // A step that is off keeps its result, whatever came before it.
        library
            .update(&call.id, |c| {
                c.completed_steps.insert("off".into(), Local::now());
            })
            .unwrap();
        pipeline.reset_from(&library, &call.id, "flaky").unwrap();
        let reset = library.load(&call.id).unwrap();
        assert_eq!(reset.status, Status::Pending);
        assert!(reset.completed_steps.contains_key("heavy"));
        assert!(reset.completed_steps.contains_key("off"));
        assert!(!reset.completed_steps.contains_key("flaky"));

        // Not while it is at it, and only steps that are on.
        assert!(pipeline.reset_from(&library, &call.id, "flaky").is_err());
        library.update(&call.id, |c| c.status = Status::Ready).unwrap();
        assert!(pipeline.reset_from(&library, &call.id, "off").is_err());

        pipeline.reset_from(&library, &call.id, "heavy").unwrap();
        assert!(!library.load(&call.id).unwrap().completed_steps.contains_key("heavy"));
        fs::remove_dir_all(tmp).unwrap();
    }
}
