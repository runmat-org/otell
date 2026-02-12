use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpanRecord {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: String,
    pub name: String,
    pub start_ts: DateTime<Utc>,
    pub end_ts: DateTime<Utc>,
    pub status: String,
    pub attrs_json: String,
    pub events_json: String,
}

impl SpanRecord {
    pub fn duration_ms(&self) -> i64 {
        (self.end_ts - self.start_ts).num_milliseconds().max(0)
    }
}
