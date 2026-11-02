//! Per-camera audio: a mixer fed by each session, played through cpal.

pub mod mixer;
pub mod output;

pub use mixer::Mixer;
pub use output::AudioOutput;
