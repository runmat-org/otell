use chrono::{Duration, TimeZone, Utc};
use otell_core::model::log::LogRecord;
use otell_core::model::span::SpanRecord;

pub fn sample_trace(trace_id: &str) -> (Vec<SpanRecord>, Vec<LogRecord>) {
    let base = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
    let spans = vec![
        SpanRecord {
            trace_id: trace_id.to_string(),
            span_id: "root".to_string(),
            parent_span_id: None,
            service: "api".to_string(),
            name: "GET /v1/orders".to_string(),
            start_ts: base,
            end_ts: base + Duration::milliseconds(1800),
            status: "ERROR".to_string(),
            attrs_json: "{}".to_string(),
            events_json: "[]".to_string(),
        },
        SpanRecord {
            trace_id: trace_id.to_string(),
            span_id: "child".to_string(),
            parent_span_id: Some("root".to_string()),
            service: "api".to_string(),
            name: "cache.get redis".to_string(),
            start_ts: base + Duration::milliseconds(900),
            end_ts: base + Duration::milliseconds(1600),
            status: "ERROR".to_string(),
            attrs_json: "{\"peer\":\"redis:6379\"}".to_string(),
            events_json: "[]".to_string(),
        },
    ];

    let logs = vec![
        LogRecord {
            ts: base + Duration::milliseconds(950),
            service: "api".to_string(),
            severity: 13,
            trace_id: Some(trace_id.to_string()),
            span_id: Some("child".to_string()),
            body: "retrying attempt=2".to_string(),
            attrs_json: "{}".to_string(),
            attrs_text: "attempt=2".to_string(),
        },
        LogRecord {
            ts: base + Duration::milliseconds(1200),
            service: "api".to_string(),
            severity: 17,
            trace_id: Some(trace_id.to_string()),
            span_id: Some("child".to_string()),
            body: "context deadline exceeded".to_string(),
            attrs_json: "{\"peer\":\"redis:6379\"}".to_string(),
            attrs_text: "peer=redis:6379".to_string(),
        },
    ];

    (spans, logs)
}
