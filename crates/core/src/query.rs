use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::filter::{AttrFilter, Severity, SortOrder, TimeWindow};
use crate::model::log::LogRecord;
use crate::model::metric::MetricPoint;
use crate::model::span::SpanRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub pattern: Option<String>,
    pub fixed: bool,
    pub ignore_case: bool,
    pub service: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub severity_gte: Option<Severity>,
    pub attr_filters: Vec<AttrFilter>,
    pub window: TimeWindow,
    pub sort: SortOrder,
    pub limit: usize,
    pub context_lines: usize,
    pub context_seconds: Option<i64>,
    pub count_only: bool,
    pub include_stats: bool,
}

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            pattern: None,
            fixed: false,
            ignore_case: false,
            service: None,
            trace_id: None,
            span_id: None,
            severity_gte: None,
            attr_filters: Vec::new(),
            window: TimeWindow::all(),
            sort: SortOrder::TsAsc,
            limit: 100,
            context_lines: 0,
            context_seconds: None,
            count_only: false,
            include_stats: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchStats {
    pub by_service: Vec<(String, usize)>,
    pub by_severity: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub total_matches: usize,
    pub returned: usize,
    pub records: Vec<LogRecord>,
    pub stats: Option<SearchStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogContextMode {
    None,
    Bounded,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRequest {
    pub trace_id: String,
    pub root_span_id: Option<String>,
    pub logs: LogContextMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsContextMeta {
    pub policy: String,
    pub limit: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResponse {
    pub trace_id: String,
    pub spans: Vec<SpanRecord>,
    pub logs: Vec<LogRecord>,
    pub context: LogsContextMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanRequest {
    pub trace_id: String,
    pub span_id: String,
    pub logs: LogContextMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanResponse {
    pub span: SpanRecord,
    pub logs: Vec<LogRecord>,
    pub context: LogsContextMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracesRequest {
    pub service: Option<String>,
    pub status: Option<String>,
    pub window: TimeWindow,
    pub sort: SortOrder,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceListItem {
    pub trace_id: String,
    pub root_name: String,
    pub duration_ms: i64,
    pub span_count: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsRequest {
    pub name: String,
    pub service: Option<String>,
    pub window: TimeWindow,
    pub group_by: Option<String>,
    pub agg: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSeries {
    pub group: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResponse {
    pub points: Vec<MetricPoint>,
    pub series: Vec<MetricSeries>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsListRequest {
    pub service: Option<String>,
    pub window: TimeWindow,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricNameItem {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsListResponse {
    pub metrics: Vec<MetricNameItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub db_path: String,
    pub db_size_bytes: u64,
    pub logs_count: usize,
    pub spans_count: usize,
    pub metrics_count: usize,
    pub oldest_ts: Option<DateTime<Utc>>,
    pub newest_ts: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHandle {
    pub handle: String,
}
