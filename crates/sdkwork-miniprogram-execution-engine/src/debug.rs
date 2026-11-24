use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum number of debug records retained for one application.
pub const DEFAULT_DEBUG_EVENT_LIMIT: usize = 1_024;

/// Runtime-owned debug event category exposed to Inspector clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DebugEventKind {
    /// JavaScript console output or an uncaught runtime diagnostic.
    Console,
    /// A host-backed mini-program network operation.
    Network,
    /// Application or page lifecycle activity.
    Lifecycle,
    /// Inspector protocol activity.
    Inspector,
}

/// One bounded, sequence-numbered runtime debug event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebugEvent {
    /// Monotonic sequence number scoped to one application.
    pub sequence: u64,
    /// Event time in Unix milliseconds when available.
    pub timestamp_ms: u64,
    /// Event category used by Inspector panels for filtering.
    pub kind: DebugEventKind,
    /// Structured event payload. Sensitive fields must be redacted by the provider.
    pub payload: Value,
}

/// Bounded event journal shared by Runtime Inspector consumers.
#[derive(Debug)]
pub struct DebugJournal {
    limit: usize,
    next_sequence: u64,
    events: VecDeque<DebugEvent>,
}

impl Default for DebugJournal {
    fn default() -> Self {
        Self::new(DEFAULT_DEBUG_EVENT_LIMIT)
    }
}

impl DebugJournal {
    /// Create a journal with a bounded event capacity.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            next_sequence: 1,
            events: VecDeque::new(),
        }
    }

    /// Append a runtime event and return its assigned sequence number.
    pub fn record(&mut self, kind: DebugEventKind, payload: Value) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.events.push_back(DebugEvent {
            sequence,
            timestamp_ms: now_millis(),
            kind,
            payload,
        });
        while self.events.len() > self.limit {
            self.events.pop_front();
        }
        sequence
    }

    /// Return events after an optional sequence cursor.
    #[must_use]
    pub fn snapshot_since(&self, after: Option<u64>) -> Vec<DebugEvent> {
        self.events
            .iter()
            .filter(|event| after.is_none_or(|cursor| event.sequence > cursor))
            .cloned()
            .collect()
    }

    /// Remove all retained events while preserving sequence monotonicity.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{DebugEventKind, DebugJournal};
    use serde_json::json;

    #[test]
    fn journal_is_bounded_and_cursorable() {
        let mut journal = DebugJournal::new(2);
        let first = journal.record(DebugEventKind::Console, json!({"message":"one"}));
        journal.record(
            DebugEventKind::Network,
            json!({"url":"https://example.test"}),
        );
        let third = journal.record(DebugEventKind::Lifecycle, json!({"state":"Visible"}));

        let events = journal.snapshot_since(Some(first));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence, first + 1);
        assert_eq!(events[1].sequence, third);
    }
}
