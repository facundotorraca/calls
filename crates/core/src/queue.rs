use std::sync::mpsc::{self, Sender};
use std::thread;

use crate::CallId;

/// A call to process, and whether the user asked for it (see `StepContext::on_demand`).
pub type Job = (CallId, bool);

/// Processes calls one at a time on a background thread.
///
/// Sequential on purpose: transcription already saturates the GPU, running two at once
/// would only make both slower.
#[derive(Clone)]
pub struct Queue {
    tx: Sender<Job>,
}

impl Queue {
    pub fn spawn(process: impl Fn(&CallId, bool) + Send + 'static) -> Queue {
        let (tx, rx) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("calls-queue".into())
            .spawn(move || {
                for (id, on_demand) in rx {
                    process(&id, on_demand);
                }
            })
            .expect("failed to start the processing thread");
        Queue { tx }
    }

    /// A call that just ended or was imported.
    pub fn push(&self, id: CallId) {
        // Only fails if the worker thread died, in which case there is nobody left to report to.
        let _ = self.tx.send((id, false));
    }

    /// A call the user asked to process.
    pub fn push_on_demand(&self, id: CallId) {
        let _ = self.tx.send((id, true));
    }
}
