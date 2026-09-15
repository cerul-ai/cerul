use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Whole indexing invocation, including all selected videos and streams.
    /// ETA is an approximate duration from priors, local history and observed work.
    IndexProgress {
        episode: String,
        source: Option<PathBuf>,
        phase: String,
        done: u64,
        /// Upper bound for a human estimate within active work; done stays measured.
        #[serde(default)]
        ceiling: u64,
        total: u64,
        eta_seconds: f64,
        finished: bool,
    },
    /// Per-attempt diagnostics, with unknown usage retained as null.
    ModelRequest {
        #[serde(flatten)]
        report: crate::providers::usage::RequestReport,
    },
    /// Completed annotation work units, including reused units separately.
    AnnotationProgress {
        episode: String,
        phase: String,
        done: u64,
        total: u64,
        cached: u64,
    },
    /// Provisional answer text. Only the final report confirms validated success.
    AnalysisDelta {
        episode: String,
        stream: String,
        text: String,
        cached: bool,
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
