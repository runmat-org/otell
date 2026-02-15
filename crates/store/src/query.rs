use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use duckdb::{params, params_from_iter};
use otell_core::error::{OtellError, Result};
use otell_core::filter::SortOrder;
use otell_core::model::log::LogRecord;
use otell_core::model::metric::MetricPoint;
use otell_core::model::span::SpanRecord;
use otell_core::query::{
    LogContextMode, LogsContextMeta, MetricNameItem, MetricSeries, MetricsListRequest,
    MetricsListResponse, MetricsRequest, MetricsResponse, SearchRequest, SearchResponse,
    SearchStats, SpanRequest, SpanResponse, TraceListItem, TraceRequest, TraceResponse,
    TracesRequest,
};
use regex::RegexBuilder;

use crate::Store;

impl Store {
    pub fn search_logs(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let candidates = self.fetch_logs_candidates(req)?;
        let filtered = apply_pattern(candidates, req)?;
        let total_matches = filtered.len();
        let stats = req.include_stats.then(|| compute_search_stats(&filtered));

        if req.count_only {
            return Ok(SearchResponse {
                total_matches,
                returned: 0,
                records: Vec::new(),
                stats,
            });
        }

        let mut selected = filtered.into_iter().take(req.limit).collect::<Vec<_>>();
        if req.context_lines > 0 {
            selected = self.expand_with_context(&selected, req.context_lines)?;
        }
        if let Some(seconds) = req.context_seconds {
            selected = self.expand_with_time_context(&selected, seconds)?;
        }

        Ok(SearchResponse {
            total_matches,
            returned: selected.len(),
            records: selected,
            stats,
        })
    }

    pub fn get_trace(&self, req: &TraceRequest) -> Result<TraceResponse> {
        let spans = self.fetch_trace_spans(&req.trace_id)?;
        let spans = if let Some(root) = &req.root_span_id {
            filter_subtree(spans, root)
        } else {
            spans
        };

        let logs = match req.logs {
            LogContextMode::None => Vec::new(),
            LogContextMode::All => self.fetch_logs_for_trace(&req.trace_id, usize::MAX)?,
            LogContextMode::Bounded => {
                self.fetch_logs_for_trace_bounded(&req.trace_id, &spans, 50)?
            }
        };

        let truncated = matches!(req.logs, LogContextMode::Bounded) && logs.len() >= 50;
        Ok(TraceResponse {
            trace_id: req.trace_id.clone(),
            spans,
            logs,
            context: LogsContextMeta {
                policy: match req.logs {
                    LogContextMode::None => "none",
                    LogContextMode::All => "all",
                    LogContextMode::Bounded => "bounded",
                }
                .to_string(),
                limit: 50,
                truncated,
            },
        })
    }

    pub fn get_span(&self, req: &SpanRequest) -> Result<SpanResponse> {
        let trace = self.get_trace(&TraceRequest {
            trace_id: req.trace_id.clone(),
            root_span_id: None,
            logs: LogContextMode::None,
        })?;

        let span = trace
            .spans
            .into_iter()
            .find(|s| s.span_id == req.span_id)
            .ok_or_else(|| OtellError::Store(format!("span not found: {}", req.span_id)))?;

        let logs = match req.logs {
            LogContextMode::None => Vec::new(),
            LogContextMode::All => {
                let mut all = self.fetch_logs_for_trace(&req.trace_id, usize::MAX)?;
                all.retain(|l| l.span_id.as_deref() == Some(req.span_id.as_str()));
                all
            }
            LogContextMode::Bounded => {
                self.fetch_logs_around_span(&req.trace_id, &req.span_id, 30)?
            }
        };

        let truncated = matches!(req.logs, LogContextMode::Bounded) && logs.len() == 30;

        Ok(SpanResponse {
            span,
            logs,
            context: LogsContextMeta {
                policy: match req.logs {
                    LogContextMode::None => "none",
                    LogContextMode::All => "all",
                    LogContextMode::Bounded => "bounded",
                }
                .to_string(),
                limit: 30,
                truncated,
            },
        })
    }

    pub fn list_traces(&self, req: &TracesRequest) -> Result<Vec<TraceListItem>> {
        let conn = self.conn();
        let sql = if req.service.is_some() {
            "SELECT s.trace_id, s.name, s.start_ts, s.end_ts, s.status,
                    (SELECT COUNT(*) FROM spans s2 WHERE s2.trace_id = s.trace_id) AS span_count
             FROM spans s
             WHERE s.parent_span_id IS NULL
               AND EXISTS (
                 SELECT 1 FROM spans sf WHERE sf.trace_id = s.trace_id AND sf.service = ?
               )"
        } else {
            "SELECT s.trace_id, s.name, s.start_ts, s.end_ts, s.status,
                    (SELECT COUNT(*) FROM spans s2 WHERE s2.trace_id = s.trace_id) AS span_count
             FROM spans s
             WHERE s.parent_span_id IS NULL"
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| OtellError::Store(format!("prepare traces failed: {e}")))?;

        let tuples = if let Some(service) = &req.service {
            let rows = stmt
                .query_map(params![service], |row| {
                    let trace_id = row.get::<_, String>(0)?;
                    let root_name = row.get::<_, String>(1)?;
                    let start = naive_to_utc(row.get::<_, NaiveDateTime>(2)?);
                    let end = naive_to_utc(row.get::<_, NaiveDateTime>(3)?);
                    let status = row.get::<_, String>(4)?;
                    let span_count = row.get::<_, i64>(5)? as usize;
                    Ok((trace_id, root_name, start, end, status, span_count))
                })
                .map_err(|e| OtellError::Store(format!("query traces failed: {e}")))?;

            let mut out = Vec::new();
            for row in rows {
                out.push(
                    row.map_err(|e| OtellError::Store(format!("map traces row failed: {e}")))?,
                );
            }
            out
        } else {
            let rows = stmt
                .query_map([], |row| {
                    let trace_id = row.get::<_, String>(0)?;
                    let root_name = row.get::<_, String>(1)?;
                    let start = naive_to_utc(row.get::<_, NaiveDateTime>(2)?);
                    let end = naive_to_utc(row.get::<_, NaiveDateTime>(3)?);
                    let status = row.get::<_, String>(4)?;
                    let span_count = row.get::<_, i64>(5)? as usize;
                    Ok((trace_id, root_name, start, end, status, span_count))
                })
                .map_err(|e| OtellError::Store(format!("query traces failed: {e}")))?;

            let mut out = Vec::new();
            for row in rows {
                out.push(
                    row.map_err(|e| OtellError::Store(format!("map traces row failed: {e}")))?,
                );
            }
            out
        };

        let mut items = Vec::new();
        for (trace_id, root_name, start, end, status, span_count) in tuples {
            if !in_window(start, &req.window.since, &req.window.until) {
                continue;
            }
            if let Some(filter_status) = &req.status
                && status != *filter_status
            {
                continue;
            }
            items.push(TraceListItem {
                trace_id,
                root_name,
                duration_ms: (end - start).num_milliseconds(),
                span_count,
                status,
            });
        }

        match req.sort {
            SortOrder::DurationDesc => items.sort_by_key(|i| Reverse(i.duration_ms)),
            SortOrder::TsAsc => items.sort_by_key(|i| i.duration_ms),
            SortOrder::TsDesc => items.sort_by_key(|i| Reverse(i.duration_ms)),
        }

        items.truncate(req.limit);
        Ok(items)
    }

    pub fn query_metrics(&self, req: &MetricsRequest) -> Result<MetricsResponse> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT ts, name, service, value, attrs_json
                 FROM metric_points
                 WHERE name = ?
                 ORDER BY ts ASC",
            )
            .map_err(|e| OtellError::Store(format!("prepare metric query failed: {e}")))?;

        let rows = stmt
            .query_map(params![req.name], |row| {
                Ok(MetricPoint {
                    ts: naive_to_utc(row.get::<_, NaiveDateTime>(0)?),
                    name: row.get::<_, String>(1)?,
                    service: row.get::<_, String>(2)?,
                    value: row.get::<_, f64>(3)?,
                    attrs_json: row.get::<_, String>(4)?,
                })
            })
            .map_err(|e| OtellError::Store(format!("query metrics failed: {e}")))?;

        let mut points = Vec::new();
        for row in rows {
            let p = row.map_err(|e| OtellError::Store(format!("map metrics row failed: {e}")))?;
            if !in_window(p.ts, &req.window.since, &req.window.until) {
                continue;
            }
            if let Some(service) = &req.service
                && &p.service != service
            {
                continue;
            }
            points.push(p);
        }

        let series = aggregate_metrics(
            &points,
            req.group_by.as_deref(),
            req.agg.as_deref(),
            req.limit,
        );
        Ok(MetricsResponse { points, series })
    }

    pub fn list_metric_names(&self, req: &MetricsListRequest) -> Result<MetricsListResponse> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT ts, name, service FROM metric_points ORDER BY ts DESC")
            .map_err(|e| OtellError::Store(format!("prepare metric names failed: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let ts = naive_to_utc(row.get::<_, NaiveDateTime>(0)?);
                let name = row.get::<_, String>(1)?;
                let service = row.get::<_, String>(2)?;
                Ok((ts, name, service))
            })
            .map_err(|e| OtellError::Store(format!("query metric names failed: {e}")))?;

        let mut counts: HashMap<String, usize> = HashMap::new();
        for row in rows {
            let (ts, name, service) =
                row.map_err(|e| OtellError::Store(format!("map metric names row failed: {e}")))?;
            if !in_window(ts, &req.window.since, &req.window.until) {
                continue;
            }
            if let Some(filter) = &req.service
                && &service != filter
            {
                continue;
            }
            *counts.entry(name).or_insert(0) += 1;
        }

        let mut metrics = counts
            .into_iter()
            .map(|(name, count)| MetricNameItem { name, count })
            .collect::<Vec<_>>();
        metrics.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        metrics.truncate(req.limit);

        Ok(MetricsListResponse { metrics })
    }

    fn fetch_logs_candidates(&self, req: &SearchRequest) -> Result<Vec<LogRecord>> {
        let conn = self.conn();

        let mut where_parts = Vec::new();
        let mut args: Vec<duckdb::types::Value> = Vec::new();

        if let Some(service) = &req.service {
            where_parts.push("service = ?");
            args.push(duckdb::types::Value::Text(service.clone()));
        }
        if let Some(trace_id) = &req.trace_id {
            where_parts.push("trace_id = ?");
            args.push(duckdb::types::Value::Text(trace_id.clone()));
        }
        if let Some(span_id) = &req.span_id {
            where_parts.push("span_id = ?");
            args.push(duckdb::types::Value::Text(span_id.clone()));
        }
        if let Some(severity) = req.severity_gte {
            where_parts.push("severity >= ?");
            args.push(duckdb::types::Value::Int(severity as i32));
        }
        if let Some(since) = req.window.since {
            where_parts.push("ts >= ?");
            args.push(duckdb::types::Value::Text(since.to_rfc3339()));
        }
        if let Some(until) = req.window.until {
            where_parts.push("ts <= ?");
            args.push(duckdb::types::Value::Text(until.to_rfc3339()));
        }

        let where_sql = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };

        let sql = format!(
            "SELECT ts, service, severity, trace_id, span_id, body, attrs_json, attrs_text
             FROM logs
             {where_sql}
             ORDER BY ts ASC"
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| OtellError::Store(format!("prepare search failed: {e}")))?;

        let rows = stmt
            .query_map(params_from_iter(args.iter()), |row| {
                Ok(LogRecord {
                    ts: naive_to_utc(row.get::<_, NaiveDateTime>(0)?),
                    service: row.get::<_, String>(1)?,
                    severity: row.get::<_, i32>(2)?,
                    trace_id: row.get::<_, Option<String>>(3)?,
                    span_id: row.get::<_, Option<String>>(4)?,
                    body: row.get::<_, String>(5)?,
                    attrs_json: row.get::<_, String>(6)?,
                    attrs_text: row.get::<_, String>(7)?,
                })
            })
            .map_err(|e| OtellError::Store(format!("query search failed: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            let record =
                row.map_err(|e| OtellError::Store(format!("map search row failed: {e}")))?;
            if !matches_attr_filters(&record.attrs_json, &req.attr_filters) {
                continue;
            }
            results.push(record);
        }

        if matches!(req.sort, SortOrder::TsDesc) {
            results.reverse();
        }

        Ok(results)
    }

    fn fetch_trace_spans(&self, trace_id: &str) -> Result<Vec<SpanRecord>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT trace_id, span_id, parent_span_id, service, name, start_ts, end_ts, status, attrs_json, events_json
                 FROM spans
                 WHERE trace_id = ?
                 ORDER BY start_ts ASC",
            )
            .map_err(|e| OtellError::Store(format!("prepare trace spans failed: {e}")))?;

        let rows = stmt
            .query_map(params![trace_id], |row| {
                Ok(SpanRecord {
                    trace_id: row.get::<_, String>(0)?,
                    span_id: row.get::<_, String>(1)?,
                    parent_span_id: row.get::<_, Option<String>>(2)?,
                    service: row.get::<_, String>(3)?,
                    name: row.get::<_, String>(4)?,
                    start_ts: naive_to_utc(row.get::<_, NaiveDateTime>(5)?),
                    end_ts: naive_to_utc(row.get::<_, NaiveDateTime>(6)?),
                    status: row.get::<_, String>(7)?,
                    attrs_json: row.get::<_, String>(8)?,
                    events_json: row.get::<_, String>(9)?,
                })
            })
            .map_err(|e| OtellError::Store(format!("query trace spans failed: {e}")))?;

        let mut spans = Vec::new();
        for row in rows {
            spans.push(row.map_err(|e| OtellError::Store(format!("map trace span failed: {e}")))?);
        }
        Ok(spans)
    }

    fn fetch_logs_for_trace(&self, trace_id: &str, limit: usize) -> Result<Vec<LogRecord>> {
        let req = SearchRequest {
            trace_id: Some(trace_id.to_string()),
            limit,
            ..SearchRequest::default()
        };
        let mut records = self.fetch_logs_candidates(&req)?;
        records.truncate(limit);
        Ok(records)
    }

    fn fetch_logs_around_span(
        &self,
        trace_id: &str,
        span_id: &str,
        limit: usize,
    ) -> Result<Vec<LogRecord>> {
        let spans = self.fetch_trace_spans(trace_id)?;
        let span = spans
            .iter()
            .find(|s| s.span_id == span_id)
            .ok_or_else(|| OtellError::Store(format!("span not found: {span_id}")))?;

        let lower = span.start_ts - Duration::seconds(1);
        let upper = span.end_ts + Duration::seconds(1);

        let req = SearchRequest {
            trace_id: Some(trace_id.to_string()),
            sort: SortOrder::TsAsc,
            limit: usize::MAX,
            ..SearchRequest::default()
        };
        let mut rows = self.fetch_logs_candidates(&req)?;
        rows.retain(|l| l.ts >= lower && l.ts <= upper);
        rows.truncate(limit);
        Ok(rows)
    }

    fn fetch_logs_for_trace_bounded(
        &self,
        trace_id: &str,
        spans: &[SpanRecord],
        limit: usize,
    ) -> Result<Vec<LogRecord>> {
        let all_logs = self.fetch_logs_for_trace(trace_id, usize::MAX)?;
        if all_logs.len() <= limit {
            return Ok(all_logs);
        }

        let mut anchors = Vec::new();
        if let Some(root) = spans.iter().find(|s| s.parent_span_id.is_none()) {
            anchors.push(root.start_ts);
            anchors.push(root.end_ts);
        }

        for s in spans.iter().filter(|s| s.status == "ERROR") {
            anchors.push(s.start_ts);
            anchors.push(s.end_ts);
        }

        let mut slow = spans.to_vec();
        slow.sort_by_key(|s| Reverse(s.duration_ms()));
        for s in slow.into_iter().take(2) {
            anchors.push(s.start_ts);
            anchors.push(s.end_ts);
        }

        let mut chosen = Vec::new();
        for anchor in anchors {
            let lower = anchor - Duration::seconds(1);
            let upper = anchor + Duration::seconds(1);
            for l in &all_logs {
                if l.ts >= lower && l.ts <= upper {
                    chosen.push(l.clone());
                }
            }
        }

        dedupe_logs(&mut chosen);
        if chosen.len() <= limit {
            return Ok(chosen);
        }

        let half = limit / 2;
        let mut out = Vec::with_capacity(limit);
        out.extend(chosen.iter().take(half).cloned());
        out.extend(chosen.iter().rev().take(limit - half).cloned().rev());
        Ok(out)
    }

    fn expand_with_context(
        &self,
        selected: &[LogRecord],
        context_lines: usize,
    ) -> Result<Vec<LogRecord>> {
        if selected.is_empty() {
            return Ok(Vec::new());
        }

        let req = SearchRequest {
            limit: usize::MAX,
            ..SearchRequest::default()
        };
        let all = self.fetch_logs_candidates(&req)?;
        let ids = selected
            .iter()
            .map(|l| (l.ts, l.body.clone(), l.span_id.clone()))
            .collect::<HashSet<_>>();

        let mut keep = HashSet::new();
        for (idx, row) in all.iter().enumerate() {
            if ids.contains(&(row.ts, row.body.clone(), row.span_id.clone())) {
                let start = idx.saturating_sub(context_lines);
                let end = (idx + context_lines + 1).min(all.len());
                for i in start..end {
                    keep.insert(i);
                }
            }
        }

        let mut output = Vec::new();
        for (idx, row) in all.iter().enumerate() {
            if keep.contains(&idx) {
                output.push(row.clone());
            }
        }
        Ok(output)
    }

    fn expand_with_time_context(
        &self,
        selected: &[LogRecord],
        seconds: i64,
    ) -> Result<Vec<LogRecord>> {
        if selected.is_empty() || seconds <= 0 {
            return Ok(selected.to_vec());
        }

        let req = SearchRequest {
            limit: usize::MAX,
            ..SearchRequest::default()
        };
        let all = self.fetch_logs_candidates(&req)?;
        let mut keep = Vec::new();

        for row in &all {
            let mut in_window_for_any = false;
            for m in selected {
                let delta_ms = (row.ts - m.ts).num_milliseconds().abs();
                if delta_ms <= seconds * 1000 {
                    in_window_for_any = true;
                    break;
                }
            }
            if in_window_for_any {
                keep.push(row.clone());
            }
        }

        dedupe_logs(&mut keep);
        Ok(keep)
    }
}

fn compute_search_stats(records: &[LogRecord]) -> SearchStats {
    let mut by_service: HashMap<String, usize> = HashMap::new();
    let mut by_severity: HashMap<String, usize> = HashMap::new();
    for record in records {
        *by_service.entry(record.service.clone()).or_insert(0) += 1;
        *by_severity
            .entry(severity_label(record.severity).to_string())
            .or_insert(0) += 1;
    }

    let mut svc = by_service.into_iter().collect::<Vec<_>>();
    svc.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut sev = by_severity.into_iter().collect::<Vec<_>>();
    sev.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    SearchStats {
        by_service: svc,
        by_severity: sev,
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

fn aggregate_metrics(
    points: &[MetricPoint],
    group_by: Option<&str>,
    agg: Option<&str>,
    limit: usize,
) -> Vec<MetricSeries> {
    let mut groups: HashMap<String, Vec<f64>> = HashMap::new();
    for p in points {
        let group = if group_by == Some("service") {
            p.service.clone()
        } else {
            "all".to_string()
        };
        groups.entry(group).or_default().push(p.value);
    }

    let mut series = groups
        .into_iter()
        .map(|(group, mut values)| {
            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let value = match agg.unwrap_or("avg") {
                "count" => values.len() as f64,
                "min" => *values.first().unwrap_or(&0.0),
                "max" => *values.last().unwrap_or(&0.0),
                "p50" => percentile(&values, 0.50),
                "p95" => percentile(&values, 0.95),
                "p99" => percentile(&values, 0.99),
                _ => {
                    if values.is_empty() {
                        0.0
                    } else {
                        values.iter().sum::<f64>() / values.len() as f64
                    }
                }
            };
            MetricSeries { group, value }
        })
        .collect::<Vec<_>>();

    series.sort_by(|a, b| a.group.cmp(&b.group));
    series.truncate(limit);
    series
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn filter_subtree(spans: Vec<SpanRecord>, root: &str) -> Vec<SpanRecord> {
    let mut children: HashMap<Option<String>, Vec<String>> = HashMap::new();
    let mut map: HashMap<String, SpanRecord> = HashMap::new();
    for span in spans {
        children
            .entry(span.parent_span_id.clone())
            .or_default()
            .push(span.span_id.clone());
        map.insert(span.span_id.clone(), span);
    }

    let mut keep = HashSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(id) = stack.pop() {
        if keep.insert(id.clone())
            && let Some(next) = children.get(&Some(id))
        {
            stack.extend(next.iter().cloned());
        }
    }

    let mut out = map
        .into_iter()
        .filter_map(|(id, span)| keep.contains(&id).then_some(span))
        .collect::<Vec<_>>();
    out.sort_by_key(|s| s.start_ts);
    out
}

fn naive_to_utc(ts: NaiveDateTime) -> DateTime<Utc> {
    ts.and_utc()
}

fn in_window(
    ts: DateTime<Utc>,
    since: &Option<DateTime<Utc>>,
    until: &Option<DateTime<Utc>>,
) -> bool {
    if let Some(since) = since
        && ts < *since
    {
        return false;
    }
    if let Some(until) = until
        && ts > *until
    {
        return false;
    }
    true
}

fn matches_attr_filters(attrs_json: &str, filters: &[otell_core::filter::AttrFilter]) -> bool {
    if filters.is_empty() {
        return true;
    }

    let parsed =
        serde_json::from_str::<serde_json::Value>(attrs_json).unwrap_or(serde_json::Value::Null);
    for filter in filters {
        let key = filter.key.trim_start_matches("attrs.");
        let value = parsed.get(key).and_then(|v| v.as_str()).unwrap_or_default();
        if !filter.matches(value) {
            return false;
        }
    }
    true
}

fn apply_pattern(mut rows: Vec<LogRecord>, req: &SearchRequest) -> Result<Vec<LogRecord>> {
    let Some(pattern) = &req.pattern else {
        return Ok(rows);
    };

    if req.fixed {
        let needle = if req.ignore_case {
            pattern.to_ascii_lowercase()
        } else {
            pattern.to_string()
        };
        rows.retain(|r| {
            let haystack = if req.ignore_case {
                r.body.to_ascii_lowercase()
            } else {
                r.body.clone()
            };
            haystack.contains(&needle)
        });
        return Ok(rows);
    }

    let regex = RegexBuilder::new(pattern)
        .case_insensitive(req.ignore_case)
        .build()
        .map_err(|e| OtellError::Parse(format!("invalid regex pattern: {e}")))?;

    rows.retain(|r| regex.is_match(&r.body));
    Ok(rows)
}

fn dedupe_logs(logs: &mut Vec<LogRecord>) {
    let mut seen = HashSet::new();
    logs.retain(|l| seen.insert((l.ts, l.body.clone(), l.span_id.clone())));
    logs.sort_by_key(|l| l.ts);
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use otell_core::filter::{AttrFilter, Severity, SortOrder, TimeWindow};
    use otell_core::model::log::LogRecord;
    use otell_core::model::metric::MetricPoint;
    use otell_core::model::span::SpanRecord;
    use otell_core::query::{
        LogContextMode, MetricsRequest, SearchRequest, TraceRequest, TracesRequest,
    };

    use crate::Store;

    #[test]
    fn search_filters_and_pattern() {
        let store = Store::open_in_memory().unwrap();
        let ts = chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        store
            .insert_logs(&[
                LogRecord {
                    ts,
                    service: "api".into(),
                    severity: 17,
                    trace_id: Some("t1".into()),
                    span_id: Some("s1".into()),
                    body: "timeout from redis".into(),
                    attrs_json: "{\"peer\":\"redis:6379\"}".into(),
                    attrs_text: "peer=redis:6379".into(),
                },
                LogRecord {
                    ts: ts + chrono::Duration::seconds(1),
                    service: "api".into(),
                    severity: 9,
                    trace_id: Some("t1".into()),
                    span_id: Some("s1".into()),
                    body: "healthy".into(),
                    attrs_json: "{}".into(),
                    attrs_text: "".into(),
                },
            ])
            .unwrap();

        let req = SearchRequest {
            pattern: Some("timeout".into()),
            ..SearchRequest::default()
        };
        let res = store.search_logs(&req).unwrap();
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.records[0].body, "timeout from redis");
    }

    #[test]
    fn bounded_trace_context_limits_output() {
        let store = Store::open_in_memory().unwrap();
        let base = chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        let spans = vec![SpanRecord {
            trace_id: "t1".into(),
            span_id: "root".into(),
            parent_span_id: None,
            service: "api".into(),
            name: "root".into(),
            start_ts: base,
            end_ts: base + chrono::Duration::seconds(10),
            status: "ERROR".into(),
            attrs_json: "{}".into(),
            events_json: "[]".into(),
        }];
        store.insert_spans(&spans).unwrap();

        let logs = (0..100)
            .map(|i| LogRecord {
                ts: base + chrono::Duration::milliseconds(i * 50),
                service: "api".into(),
                severity: 17,
                trace_id: Some("t1".into()),
                span_id: Some("root".into()),
                body: format!("line {i}"),
                attrs_json: "{}".into(),
                attrs_text: "".into(),
            })
            .collect::<Vec<_>>();
        store.insert_logs(&logs).unwrap();

        let trace = store
            .get_trace(&TraceRequest {
                trace_id: "t1".into(),
                root_span_id: None,
                logs: LogContextMode::Bounded,
            })
            .unwrap();
        assert!(trace.logs.len() <= 50);
        assert_eq!(trace.context.policy, "bounded");
    }

    #[test]
    fn search_attr_and_severity_filters() {
        let store = Store::open_in_memory().unwrap();
        let ts = chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        store
            .insert_logs(&[
                LogRecord {
                    ts,
                    service: "api".into(),
                    severity: 17,
                    trace_id: Some("t1".into()),
                    span_id: Some("s1".into()),
                    body: "redis timeout".into(),
                    attrs_json: "{\"peer\":\"redis:6379\"}".into(),
                    attrs_text: "peer=redis:6379".into(),
                },
                LogRecord {
                    ts: ts + chrono::Duration::seconds(1),
                    service: "api".into(),
                    severity: 9,
                    trace_id: Some("t2".into()),
                    span_id: Some("s2".into()),
                    body: "postgres timeout".into(),
                    attrs_json: "{\"peer\":\"postgres:5432\"}".into(),
                    attrs_text: "peer=postgres:5432".into(),
                },
            ])
            .unwrap();

        let req = SearchRequest {
            severity_gte: Some(Severity::Warn),
            attr_filters: vec![AttrFilter::parse("attrs.peer=redis:*").unwrap()],
            ..SearchRequest::default()
        };
        let res = store.search_logs(&req).unwrap();
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.records[0].trace_id.as_deref(), Some("t1"));
    }

    #[test]
    fn list_traces_sorts_by_duration() {
        let store = Store::open_in_memory().unwrap();
        let t0 = chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        store
            .insert_spans(&[
                SpanRecord {
                    trace_id: "t1".into(),
                    span_id: "r1".into(),
                    parent_span_id: None,
                    service: "api".into(),
                    name: "short".into(),
                    start_ts: t0,
                    end_ts: t0 + chrono::Duration::milliseconds(50),
                    status: "OK".into(),
                    attrs_json: "{}".into(),
                    events_json: "[]".into(),
                },
                SpanRecord {
                    trace_id: "t2".into(),
                    span_id: "r2".into(),
                    parent_span_id: None,
                    service: "api".into(),
                    name: "long".into(),
                    start_ts: t0,
                    end_ts: t0 + chrono::Duration::milliseconds(200),
                    status: "ERROR".into(),
                    attrs_json: "{}".into(),
                    events_json: "[]".into(),
                },
            ])
            .unwrap();

        let traces = store
            .list_traces(&TracesRequest {
                service: Some("api".into()),
                status: None,
                window: TimeWindow::all(),
                sort: SortOrder::DurationDesc,
                limit: 10,
            })
            .unwrap();

        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].trace_id, "t2");
    }

    #[test]
    fn metrics_query_aggregates() {
        let store = Store::open_in_memory().unwrap();
        let t0 = chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        store
            .insert_metrics(&[
                MetricPoint {
                    ts: t0,
                    name: "http.server.duration".into(),
                    service: "api".into(),
                    value: 10.0,
                    attrs_json: "{}".into(),
                },
                MetricPoint {
                    ts: t0 + chrono::Duration::seconds(1),
                    name: "http.server.duration".into(),
                    service: "api".into(),
                    value: 20.0,
                    attrs_json: "{}".into(),
                },
            ])
            .unwrap();

        let res = store
            .query_metrics(&MetricsRequest {
                name: "http.server.duration".into(),
                service: Some("api".into()),
                window: TimeWindow::all(),
                group_by: Some("service".into()),
                agg: Some("p95".into()),
                limit: 10,
            })
            .unwrap();

        assert_eq!(res.points.len(), 2);
        assert_eq!(res.series.len(), 1);
        assert!(res.series[0].value >= 10.0);
    }

    #[test]
    fn search_context_lines_returns_neighbors() {
        let store = Store::open_in_memory().unwrap();
        let t0 = chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        let rows = (0..5)
            .map(|i| LogRecord {
                ts: t0 + chrono::Duration::seconds(i),
                service: "api".into(),
                severity: 9,
                trace_id: None,
                span_id: None,
                body: if i == 2 {
                    "needle".into()
                } else {
                    format!("line{i}")
                },
                attrs_json: "{}".into(),
                attrs_text: "".into(),
            })
            .collect::<Vec<_>>();
        store.insert_logs(&rows).unwrap();

        let res = store
            .search_logs(&SearchRequest {
                pattern: Some("needle".into()),
                context_lines: 1,
                ..SearchRequest::default()
            })
            .unwrap();

        assert_eq!(res.records.len(), 3);
        assert_eq!(res.records[1].body, "needle");
    }

    #[test]
    fn search_count_only_with_stats() {
        let store = Store::open_in_memory().unwrap();
        let t0 = chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        store
            .insert_logs(&[
                LogRecord {
                    ts: t0,
                    service: "api".into(),
                    severity: 17,
                    trace_id: None,
                    span_id: None,
                    body: "timeout".into(),
                    attrs_json: "{}".into(),
                    attrs_text: "".into(),
                },
                LogRecord {
                    ts: t0 + chrono::Duration::seconds(1),
                    service: "api".into(),
                    severity: 13,
                    trace_id: None,
                    span_id: None,
                    body: "timeout".into(),
                    attrs_json: "{}".into(),
                    attrs_text: "".into(),
                },
            ])
            .unwrap();

        let res = store
            .search_logs(&SearchRequest {
                pattern: Some("timeout".into()),
                count_only: true,
                include_stats: true,
                ..SearchRequest::default()
            })
            .unwrap();

        assert_eq!(res.total_matches, 2);
        assert_eq!(res.returned, 0);
        assert!(res.records.is_empty());
        let stats = res.stats.unwrap();
        assert_eq!(stats.by_service[0], ("api".to_string(), 2));
    }

    #[test]
    fn search_time_context_includes_neighbors_by_time() {
        let store = Store::open_in_memory().unwrap();
        let t0 = chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        store
            .insert_logs(&[
                LogRecord {
                    ts: t0,
                    service: "api".into(),
                    severity: 9,
                    trace_id: None,
                    span_id: None,
                    body: "pre".into(),
                    attrs_json: "{}".into(),
                    attrs_text: "".into(),
                },
                LogRecord {
                    ts: t0 + chrono::Duration::milliseconds(500),
                    service: "api".into(),
                    severity: 17,
                    trace_id: None,
                    span_id: None,
                    body: "needle".into(),
                    attrs_json: "{}".into(),
                    attrs_text: "".into(),
                },
                LogRecord {
                    ts: t0 + chrono::Duration::seconds(2),
                    service: "api".into(),
                    severity: 9,
                    trace_id: None,
                    span_id: None,
                    body: "post".into(),
                    attrs_json: "{}".into(),
                    attrs_text: "".into(),
                },
            ])
            .unwrap();

        let res = store
            .search_logs(&SearchRequest {
                pattern: Some("needle".into()),
                context_seconds: Some(1),
                ..SearchRequest::default()
            })
            .unwrap();
        assert_eq!(res.records.len(), 2);
    }

    #[test]
    fn metrics_list_names() {
        let store = Store::open_in_memory().unwrap();
        let t0 = chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        store
            .insert_metrics(&[
                MetricPoint {
                    ts: t0,
                    name: "a".into(),
                    service: "api".into(),
                    value: 1.0,
                    attrs_json: "{}".into(),
                },
                MetricPoint {
                    ts: t0 + chrono::Duration::seconds(1),
                    name: "b".into(),
                    service: "api".into(),
                    value: 1.0,
                    attrs_json: "{}".into(),
                },
                MetricPoint {
                    ts: t0 + chrono::Duration::seconds(2),
                    name: "a".into(),
                    service: "api".into(),
                    value: 1.0,
                    attrs_json: "{}".into(),
                },
            ])
            .unwrap();

        let res = store
            .list_metric_names(&otell_core::query::MetricsListRequest {
                service: Some("api".into()),
                window: TimeWindow::all(),
                limit: 10,
            })
            .unwrap();
        assert_eq!(res.metrics[0].name, "a");
        assert_eq!(res.metrics[0].count, 2);
    }
}
