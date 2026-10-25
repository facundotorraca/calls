//! Core of the app: what a call is, where it lives on disk and how it gets processed.
//!
//! This crate deliberately has no knowledge of the UI (Tauri, CLI) nor of the concrete
//! tools that do the heavy lifting (ffmpeg, whisper.cpp). Those plug in through the traits
//! in [`engines`] and the [`pipeline::Step`] trait.

pub mod call;
pub mod detect;
pub mod engines;
pub mod library;
pub mod notes;
pub mod notion;
pub mod pipeline;
pub mod queue;
pub mod search;
pub mod settings;
pub mod steps;
pub mod summary;
pub mod transcript;
pub mod voices;

pub use call::{Call, CallId, Status, Track, TrackRole};
pub use library::{Folder, FolderColor, Library};
pub use pipeline::{Event, Pipeline, Step, StepContext};
pub use settings::Settings;
pub use transcript::{Segment, Transcript};
