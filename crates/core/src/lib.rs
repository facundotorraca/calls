//! Core of the app: what a call is, where it lives on disk and how it gets processed.
//!
//! This crate deliberately has no knowledge of the UI (Tauri, CLI) nor of the concrete
//! tools that do the heavy lifting (ffmpeg, whisper.cpp). Those plug in through the traits
//! in [`engines`] and the [`pipeline::Step`] trait.

pub mod call;
pub mod detect;
pub mod engines;
pub mod library;
pub mod pipeline;
pub mod queue;
pub mod settings;
pub mod steps;
pub mod transcript;

pub use call::{Call, CallId, Status, Track, TrackRole};
pub use library::Library;
pub use pipeline::{Event, Pipeline, Step, StepContext};
pub use settings::Settings;
pub use transcript::{Segment, Transcript};
