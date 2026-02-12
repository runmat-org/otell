use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogRecord {
    pub ts: DateTime<Utc>,
    pub service: String,
    pub severity: i32,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub body: String,
    pub attrs_json: String,
    pub attrs_text: String,
}
