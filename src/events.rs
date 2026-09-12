use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Per-attempt diagnostics, with unknown usage retained as null.
    ModelRequest {
        #[serde(flatten)]
        report: crate::providers::usage::RequestReport,
    },
    Progress {
        episode: String,
        station: String,
        done: u64,
        total: u64,
    },
    /// A durable unit of work that a rerun will reuse instead of repeating.
    Checkpoint {
        episode: String,
        station: String,
        window: u64,
        total: u64,
    },
    /// A validated annotation file that now exists on disk. Distinct from a
    /// checkpoint: only a published module is safe to read or train on.
    Published {
        episode: String,
        stream: String,
        annotation: String,
        records: u64,
        path: PathBuf,
    },
    Log {
        level: String,
        msg: String,
    },
}

/// Callbacks never require a terminal, making the core usable by a desktop host.
pub trait EventSink {
    fn emit(&mut self, event: Event);
}
impl<F: FnMut(Event)> EventSink for F {
    fn emit(&mut self, event: Event) {
        self(event);
    }
}
