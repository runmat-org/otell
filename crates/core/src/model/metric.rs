use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricPoint {
    pub ts: DateTime<Utc>,
    pub name: String,
    pub service: String,
    pub value: f64,
    pub attrs_json: String,
}
