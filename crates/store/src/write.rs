use duckdb::params;
use otell_core::error::{OtellError, Result};
use otell_core::model::log::LogRecord;
use otell_core::model::metric::MetricPoint;
use otell_core::model::span::SpanRecord;

use crate::Store;

impl Store {
    pub fn insert_logs(&self, logs: &[LogRecord]) -> Result<()> {
        if logs.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn();
        let tx = conn
            .transaction()
            .map_err(|e| OtellError::Store(format!("begin tx failed: {e}")))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO logs (id, ts, service, severity, trace_id, span_id, body, attrs_json, attrs_text)
                     VALUES (nextval('logs_id_seq'), ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .map_err(|e| OtellError::Store(format!("prepare insert logs failed: {e}")))?;

            for log in logs {
                stmt.execute(params![
                    log.ts.to_rfc3339(),
                    log.service,
                    log.severity,
                    log.trace_id,
                    log.span_id,
                    log.body,
                    log.attrs_json,
                    log.attrs_text,
                ])
                .map_err(|e| OtellError::Store(format!("insert log failed: {e}")))?;
            }
        }

        tx.commit()
            .map_err(|e| OtellError::Store(format!("commit logs failed: {e}")))
    }

    pub fn insert_spans(&self, spans: &[SpanRecord]) -> Result<()> {
        if spans.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn();
        let tx = conn
            .transaction()
            .map_err(|e| OtellError::Store(format!("begin tx failed: {e}")))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO spans
                     (trace_id, span_id, parent_span_id, service, name, start_ts, end_ts, status, attrs_json, events_json)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .map_err(|e| OtellError::Store(format!("prepare insert spans failed: {e}")))?;

            for span in spans {
                stmt.execute(params![
                    span.trace_id,
                    span.span_id,
                    span.parent_span_id,
                    span.service,
                    span.name,
                    span.start_ts.to_rfc3339(),
                    span.end_ts.to_rfc3339(),
                    span.status,
                    span.attrs_json,
                    span.events_json,
                ])
                .map_err(|e| OtellError::Store(format!("insert span failed: {e}")))?;
            }
        }

        tx.commit()
            .map_err(|e| OtellError::Store(format!("commit spans failed: {e}")))
    }

    pub fn insert_metrics(&self, metrics: &[MetricPoint]) -> Result<()> {
        if metrics.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn();
        let tx = conn
            .transaction()
            .map_err(|e| OtellError::Store(format!("begin tx failed: {e}")))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO metric_points (id, ts, name, service, value, attrs_json)
                     VALUES (nextval('metric_id_seq'), ?, ?, ?, ?, ?)",
                )
                .map_err(|e| OtellError::Store(format!("prepare insert metrics failed: {e}")))?;

            for metric in metrics {
                stmt.execute(params![
                    metric.ts.to_rfc3339(),
                    metric.name,
                    metric.service,
                    metric.value,
                    metric.attrs_json,
                ])
                .map_err(|e| OtellError::Store(format!("insert metric failed: {e}")))?;
            }
        }

        tx.commit()
            .map_err(|e| OtellError::Store(format!("commit metrics failed: {e}")))
    }
}
