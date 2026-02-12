pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS logs (
  id BIGINT PRIMARY KEY,
  ts TIMESTAMP NOT NULL,
  service TEXT NOT NULL,
  severity INTEGER NOT NULL,
  trace_id TEXT,
  span_id TEXT,
  body TEXT NOT NULL,
  attrs_json TEXT NOT NULL,
  attrs_text TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS spans (
  trace_id TEXT NOT NULL,
  span_id TEXT NOT NULL,
  parent_span_id TEXT,
  service TEXT NOT NULL,
  name TEXT NOT NULL,
  start_ts TIMESTAMP NOT NULL,
  end_ts TIMESTAMP NOT NULL,
  status TEXT NOT NULL,
  attrs_json TEXT NOT NULL,
  events_json TEXT NOT NULL,
  PRIMARY KEY(trace_id, span_id)
);

CREATE TABLE IF NOT EXISTS metric_points (
  id BIGINT PRIMARY KEY,
  ts TIMESTAMP NOT NULL,
  name TEXT NOT NULL,
  service TEXT NOT NULL,
  value DOUBLE NOT NULL,
  attrs_json TEXT NOT NULL
);

CREATE SEQUENCE IF NOT EXISTS logs_id_seq;
CREATE SEQUENCE IF NOT EXISTS metric_id_seq;

CREATE INDEX IF NOT EXISTS idx_logs_ts ON logs(ts);
CREATE INDEX IF NOT EXISTS idx_logs_service_ts ON logs(service, ts);
CREATE INDEX IF NOT EXISTS idx_logs_trace ON logs(trace_id);
CREATE INDEX IF NOT EXISTS idx_logs_span ON logs(span_id);

CREATE INDEX IF NOT EXISTS idx_spans_trace ON spans(trace_id);
CREATE INDEX IF NOT EXISTS idx_spans_service_start ON spans(service, start_ts);

CREATE INDEX IF NOT EXISTS idx_metrics_name_ts ON metric_points(name, ts);
CREATE INDEX IF NOT EXISTS idx_metrics_service_ts ON metric_points(service, ts);
"#;
