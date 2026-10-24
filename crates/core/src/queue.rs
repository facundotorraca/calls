use std::sync::mpsc::{self, Sender};
use std::thread;

use crate::CallId;

/// Processes calls one at a time on a background thread.
///
/// Sequential on purpose: transcription already saturates the GPU, running two at once
/// would only make both slower.
#[derive(Clone)]
pub struct Queue {
    tx: Sender<CallId>,
}

impl Queue {
    pub fn spawn(process: impl Fn(&CallId) + Send + 'static) -> Queue {
        let (tx, rx) = mpsc::channel::<CallId>();
        thread::Builder::new()
            .name("calls-queue".into())
            .spawn(move || {
                for id in rx {
                    process(&id);
                }
            })
            .expect("failed to start the processing thread");
        Queue { tx }
    }

    pub fn push(&self, id: CallId) {
        // Only fails if the worker thread died, in which case there is nobody left to report to.
        let _ = self.tx.send(id);
    }
}
