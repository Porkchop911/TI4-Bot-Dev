//! Timing and event resolution stub.

use ti4_model::*;

pub struct TimingWindow {
    pub event_id: EventId,
    pub priority: i32,
    pub resolved: bool,
}

impl TimingWindow {
    pub fn new(event_id: EventId, priority: i32) -> Self {
        Self {
            event_id,
            priority,
            resolved: false,
        }
    }
}

pub struct EventResolver {
    pub events: Vec<TimingWindow>,
    pub max_depth: i32,
    pub current_depth: i32,
}

impl EventResolver {
    pub fn new(max_depth: i32) -> Self {
        Self {
            events: Vec::new(),
            max_depth,
            current_depth: 0,
        }
    }

    pub fn resolve(&mut self) -> Result<(), anyhow::Error> {
        todo!("M03: implement event resolution")
    }
}
