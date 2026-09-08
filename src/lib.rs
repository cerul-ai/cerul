//! Cerul's reusable video processing core.
//!
//! The library receives explicit configuration and emits structured values.
//! Process arguments, terminal rendering, and exit codes belong to the CLI.
pub mod config;
pub mod episode;
pub mod events;
pub mod storage;

pub mod ocr;

pub mod media;

pub mod annotations;

pub mod index;

pub mod status;

pub mod search;

pub mod annotate;
pub mod clean;
pub mod lerobot;
pub mod providers;
