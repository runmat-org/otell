# Architecture

`otell` is a local-first single-binary observability utility.

## Runtime model

`otell run` starts:

- OTLP ingest servers
  - gRPC ingest
  - HTTP ingest (protobuf + JSON)
- Query servers
  - UDS line-JSON server
  - TCP line-JSON server
  - HTTP query API server
- Retention loop
  - periodic TTL + size pruning

All data is stored in one DuckDB database.

## Data flow

1. OTLP payload arrives via ingest endpoint.
2. OTLP payload is decoded into internal records (`LogRecord`, `SpanRecord`, `MetricPoint`).
3. Records are sent into async batch pipelines.
4. Batched writes are committed to DuckDB.
5. Optional forwarder can tee inbound OTLP payloads to an upstream collector.
6. Query requests execute deterministic store queries and return structured responses.

## Runtime telemetry and instrumentation

`otell` uses `tracing` as the single instrumentation API.

- HTTP servers (ingest + query) are instrumented with `tower-http::TraceLayer`.
- CLI/runtime logs are emitted via `tracing_subscriber` fmt layer.
- Optional OTLP export is enabled when OTEL exporter env vars are set.
- Optional in-process self-observe sink writes `otell` events/spans into DuckDB (`OTELL_SELF_OBSERVE=store|both`).
- Optional inbound forwarding can tee received OTLP requests to another collector (`OTELL_FORWARD_OTLP_*`).

This keeps one consistent signal path while allowing multiple sinks (stderr, OTLP, local store).

## Crate responsibilities

- `crates/core`
  - shared domain models
  - query request/response schemas
  - config parsing and defaults
  - filter/time/id utilities
  - typed errors

- `crates/store`
  - DuckDB schema and indexes
  - insert paths for logs/spans/metrics
  - query implementations
  - retention logic

- `crates/ingest`
  - OTLP gRPC + HTTP receivers
  - OTLP decode logic
  - async batching pipeline

- `crates/otell`
  - CLI command surface
  - query client + query servers
  - HTTP query API wiring
  - MCP stdio mode
  - output formatting

- `crates/testkit`
  - deterministic fixtures used in tests

## Storage model

Main tables:

- `logs`
- `spans`
- `metric_points`

Indexes are defined for common access paths (time, service+time, trace/span id, metric name+time).

## Local security posture

- UDS query socket is created with mode `0600` on Unix.
- Query transport is local by default; no auth layer is included.
- Intended for local development and debugging workflows.
