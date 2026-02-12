use std::collections::HashMap;

use chrono::SecondsFormat;
use otell_core::query::{
    MetricsListResponse, MetricsResponse, SearchResponse, SpanResponse, StatusResponse,
    TraceListItem, TraceResponse,
};

pub fn print_search_human(v: &SearchResponse) {
    for row in &v.records {
        let ts = row.ts.to_rfc3339_opts(SecondsFormat::Millis, true);
        let trace = row.trace_id.clone().unwrap_or_else(|| "-".to_string());
        let span = row.span_id.clone().unwrap_or_else(|| "-".to_string());
        println!(
            "{ts} {} {} trace={} span={} | {} {}",
            row.service,
            severity_label(row.severity),
            trace,
            span,
            row.body,
            row.attrs_text
        );
    }
    println!(
        "-- {} matches ({} returned) --",
        v.total_matches, v.returned
    );
    if let Some(stats) = &v.stats {
        println!("stats.by_service={:?}", stats.by_service);
        println!("stats.by_severity={:?}", stats.by_severity);
    }
}

pub fn print_trace_human(v: &TraceResponse) {
    let duration_ms = if let (Some(first), Some(last)) = (v.spans.first(), v.spans.last()) {
        (last.end_ts - first.start_ts).num_milliseconds()
    } else {
        0
    };
    let errors = v.spans.iter().filter(|s| s.status == "ERROR").count();
    println!(
        "TRACE {} duration={}ms spans={} errors={}",
        v.trace_id,
        duration_ms,
        v.spans.len(),
        errors
    );

    print_span_tree(&v.spans);
    println!(
        "logs={} limit={} truncated={}",
        v.context.policy, v.context.limit, v.context.truncated
    );
    for log in &v.logs {
        println!(
            "{} {} {} | {}",
            log.ts.to_rfc3339_opts(SecondsFormat::Millis, true),
            log.service,
            severity_label(log.severity),
            log.body
        );
    }
}

pub fn print_span_human(v: &SpanResponse) {
    println!(
        "SPAN {} service={} name={} status={} duration={}ms",
        v.span.span_id,
        v.span.service,
        v.span.name,
        v.span.status,
        v.span.duration_ms()
    );
    println!("attrs={}", v.span.attrs_json);
    println!("events={}", v.span.events_json);
    println!(
        "logs={} limit={} truncated={}",
        v.context.policy, v.context.limit, v.context.truncated
    );
    for log in &v.logs {
        println!(
            "{} {} | {}",
            log.ts.to_rfc3339_opts(SecondsFormat::Millis, true),
            severity_label(log.severity),
            log.body
        );
    }
}

pub fn print_traces_human(v: &[TraceListItem]) {
    for item in v {
        println!(
            "trace={} duration={}ms spans={} status={} root=\"{}\"",
            item.trace_id, item.duration_ms, item.span_count, item.status, item.root_name
        );
    }
    println!("-- {} traces --", v.len());
}

pub fn print_metrics_human(v: &MetricsResponse) {
    println!("points={}", v.points.len());
    for s in &v.series {
        println!("group={} value={}", s.group, s.value);
    }
    println!(
        "-- {} series ({} points) --",
        v.series.len(),
        v.points.len()
    );
}

pub fn print_metrics_list_human(v: &MetricsListResponse) {
    for metric in &v.metrics {
        println!("name={} count={}", metric.name, metric.count);
    }
    println!("-- {} metric names --", v.metrics.len());
}

pub fn print_status_human(v: &StatusResponse) {
    println!("db_path={}", v.db_path);
    println!("db_size_bytes={}", v.db_size_bytes);
    println!(
        "logs={} spans={} metrics={}",
        v.logs_count, v.spans_count, v.metrics_count
    );
    if let Some(oldest) = v.oldest_ts {
        println!(
            "oldest={}",
            oldest.to_rfc3339_opts(SecondsFormat::Millis, true)
        );
    }
    if let Some(newest) = v.newest_ts {
        println!(
            "newest={}",
            newest.to_rfc3339_opts(SecondsFormat::Millis, true)
        );
    }
}

fn severity_label(level: i32) -> &'static str {
    match level {
        1..=4 => "TRACE",
        5..=8 => "DEBUG",
        9..=12 => "INFO",
        13..=16 => "WARN",
        17..=20 => "ERROR",
        _ => "FATAL",
    }
}

fn print_span_tree(spans: &[otell_core::model::span::SpanRecord]) {
    let mut children: HashMap<Option<String>, Vec<&otell_core::model::span::SpanRecord>> =
        HashMap::new();
    for span in spans {
        children
            .entry(span.parent_span_id.clone())
            .or_default()
            .push(span);
    }
    if let Some(roots) = children.get(&None) {
        for root in roots {
            print_node(root, &children, 0);
        }
    }
}

fn print_node(
    span: &otell_core::model::span::SpanRecord,
    children: &HashMap<Option<String>, Vec<&otell_core::model::span::SpanRecord>>,
    depth: usize,
) {
    let indent = "  ".repeat(depth);
    println!(
        "{}{} {} ({}ms) {}",
        indent,
        span.service,
        span.name,
        span.duration_ms(),
        span.status
    );

    if let Some(kids) = children.get(&Some(span.span_id.clone())) {
        for child in kids {
            print_node(child, children, depth + 1);
        }
    }
}
