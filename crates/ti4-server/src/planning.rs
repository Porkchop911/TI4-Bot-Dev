//! Record a player's answers and replay them on a fresh game copy.
//!
//! The adapters and the checks of the publication boundary are defined in `ti4-view`, which the
//! wasm build uses too. The [`runner`] module puts them around a disposable engine fork on a
//! worker thread.

pub mod runner;

pub use ti4_host::planning::{
    DecisionRecording, RecordedDecision, RecordingDecider, ReplayDecider, ReplayProgress,
    ReplayStatus, ReplayStopReason,
};
