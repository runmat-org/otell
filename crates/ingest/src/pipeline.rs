use std::time::Duration;

use otell_core::model::log::LogRecord;
use otell_core::model::metric::MetricPoint;
use otell_core::model::span::SpanRecord;
use otell_store::Store;
use tokio::sync::mpsc;
use tracing::warn;

#[derive(Clone)]
pub struct Pipeline {
    logs_tx: mpsc::Sender<Vec<LogRecord>>,
    spans_tx: mpsc::Sender<Vec<SpanRecord>>,
    metrics_tx: mpsc::Sender<Vec<MetricPoint>>,
}

pub struct PipelineConfig {
    pub channel_capacity: usize,
    pub flush_interval: Duration,
    pub batch_size: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 256,
            flush_interval: Duration::from_millis(200),
            batch_size: 2048,
        }
    }
}

impl Pipeline {
    pub fn new(store: Store, cfg: PipelineConfig) -> Self {
        let (logs_tx, logs_rx) = mpsc::channel(cfg.channel_capacity);
        let (spans_tx, spans_rx) = mpsc::channel(cfg.channel_capacity);
        let (metrics_tx, metrics_rx) = mpsc::channel(cfg.channel_capacity);

        tokio::spawn(run_log_writer(
            store.clone(),
            logs_rx,
            cfg.batch_size,
            cfg.flush_interval,
        ));
        tokio::spawn(run_span_writer(
            store.clone(),
            spans_rx,
            cfg.batch_size,
            cfg.flush_interval,
        ));
        tokio::spawn(run_metric_writer(
            store,
            metrics_rx,
            cfg.batch_size,
            cfg.flush_interval,
        ));

        Self {
            logs_tx,
            spans_tx,
            metrics_tx,
        }
    }

    pub async fn submit_logs(&self, logs: Vec<LogRecord>) {
        if self.logs_tx.send(logs).await.is_err() {
            warn!("log pipeline dropped batch: receiver closed");
        }
    }

    pub async fn submit_spans(&self, spans: Vec<SpanRecord>) {
        if self.spans_tx.send(spans).await.is_err() {
            warn!("span pipeline dropped batch: receiver closed");
        }
    }

    pub async fn submit_metrics(&self, metrics: Vec<MetricPoint>) {
        if self.metrics_tx.send(metrics).await.is_err() {
            warn!("metric pipeline dropped batch: receiver closed");
        }
    }
}

async fn run_log_writer(
    store: Store,
    mut rx: mpsc::Receiver<Vec<LogRecord>>,
    batch_size: usize,
    flush_interval: Duration,
) {
    let mut ticker = tokio::time::interval(flush_interval);
    let mut buffer = Vec::new();
    loop {
        tokio::select! {
            Some(batch) = rx.recv() => {
                buffer.extend(batch);
                if buffer.len() >= batch_size {
                    flush_logs(&store, &mut buffer);
                }
            }
            _ = ticker.tick() => {
                if !buffer.is_empty() {
                    flush_logs(&store, &mut buffer);
                }
            }
            else => break,
        }
    }
}

async fn run_span_writer(
    store: Store,
    mut rx: mpsc::Receiver<Vec<SpanRecord>>,
    batch_size: usize,
    flush_interval: Duration,
) {
    let mut ticker = tokio::time::interval(flush_interval);
    let mut buffer = Vec::new();
    loop {
        tokio::select! {
            Some(batch) = rx.recv() => {
                buffer.extend(batch);
                if buffer.len() >= batch_size {
                    flush_spans(&store, &mut buffer);
                }
            }
            _ = ticker.tick() => {
                if !buffer.is_empty() {
                    flush_spans(&store, &mut buffer);
                }
            }
            else => break,
        }
    }
}

async fn run_metric_writer(
    store: Store,
    mut rx: mpsc::Receiver<Vec<MetricPoint>>,
    batch_size: usize,
    flush_interval: Duration,
) {
    let mut ticker = tokio::time::interval(flush_interval);
    let mut buffer = Vec::new();
    loop {
        tokio::select! {
            Some(batch) = rx.recv() => {
                buffer.extend(batch);
                if buffer.len() >= batch_size {
                    flush_metrics(&store, &mut buffer);
                }
            }
            _ = ticker.tick() => {
                if !buffer.is_empty() {
                    flush_metrics(&store, &mut buffer);
                }
            }
            else => break,
        }
    }
}

fn flush_logs(store: &Store, buffer: &mut Vec<LogRecord>) {
    if let Err(e) = store.insert_logs(buffer) {
        warn!(error = ?e, "failed to write log batch");
    }
    buffer.clear();
}

fn flush_spans(store: &Store, buffer: &mut Vec<SpanRecord>) {
    if let Err(e) = store.insert_spans(buffer) {
        warn!(error = ?e, "failed to write span batch");
    }
    buffer.clear();
}

fn flush_metrics(store: &Store, buffer: &mut Vec<MetricPoint>) {
    if let Err(e) = store.insert_metrics(buffer) {
        warn!(error = ?e, "failed to write metric batch");
    }
    buffer.clear();
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use otell_core::model::log::LogRecord;
    use otell_core::query::SearchRequest;

    use super::*;

    #[tokio::test]
    async fn pipeline_writes_logs() {
        let store = Store::open_in_memory().unwrap();
        let pipeline = Pipeline::new(
            store.clone(),
            PipelineConfig {
                channel_capacity: 8,
                flush_interval: std::time::Duration::from_millis(10),
                batch_size: 4,
            },
        );

        let ts = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        pipeline
            .submit_logs(vec![LogRecord {
                ts,
                service: "api".into(),
                severity: 17,
                trace_id: Some("t1".into()),
                span_id: Some("s1".into()),
                body: "error".into(),
                attrs_json: "{}".into(),
                attrs_text: "".into(),
            }])
            .await;

        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        let res = store.search_logs(&SearchRequest::default()).unwrap();
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.records[0].body, "error");
    }

    #[tokio::test]
    async fn pipeline_flushes_on_batch_size() {
        let store = Store::open_in_memory().unwrap();
        let pipeline = Pipeline::new(
            store.clone(),
            PipelineConfig {
                channel_capacity: 8,
                flush_interval: std::time::Duration::from_secs(5),
                batch_size: 2,
            },
        );

        let base = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        for i in 0..2 {
            pipeline
                .submit_logs(vec![LogRecord {
                    ts: base + Duration::seconds(i),
                    service: "api".into(),
                    severity: 9,
                    trace_id: None,
                    span_id: None,
                    body: format!("line{i}"),
                    attrs_json: "{}".into(),
                    attrs_text: "".into(),
                }])
                .await;
        }

        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        let res = store.search_logs(&SearchRequest::default()).unwrap();
        assert_eq!(res.total_matches, 2);
    }
}
