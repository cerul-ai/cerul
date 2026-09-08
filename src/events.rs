use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Progress {
        episode: String,
        station: String,
        done: u64,
        total: u64,
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
