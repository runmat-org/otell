# otell

`otell` is a single-binary local OpenTelemetry ingest and debugging utility.

It is designed for local development loops where you want a tool that can:

- receive OTLP logs, traces, and metrics
- store everything locally in DuckDB
- query with deterministic, grep-like workflows
- provide machine-friendly JSON for coding agents and MCP-style integrations

## What is implemented

- OTLP ingest
  - gRPC listener (`--otlp-grpc-addr`, default `127.0.0.1:4317`)
  - HTTP listener (`--otlp-http-addr`, default `127.0.0.1:4318`)
- Local store
  - DuckDB-backed tables for `logs`, `spans`, and `metric_points`
  - retention policies (TTL and coarse size cap)
- Query/control plane
  - UDS query server (default path from config)
  - TCP fallback query server (`--query-tcp-addr`, default `127.0.0.1:1777`)
- CLI
  - `run`, `search`, `trace`, `span`, `traces`, `metrics`, `status`, `doctor`, `mcp`
- Deterministic defaults
  - explicit sorting, filtering, and bounded trace log context
- Tests
  - unit tests across core/store/ingest
  - integration test for HTTP ingest + CLI query end-to-end

## Quick start

Start the daemon:

```bash
cargo run -p otell -- run
```

Send OTLP from your service (example env):

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
export OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
```

Query logs:

```bash
cargo run -p otell -- search "timeout" --since 15m --service api
```

Inspect a trace:

```bash
cargo run -p otell -- trace <trace_id>
```

Inspect a span:

```bash
cargo run -p otell -- span <trace_id> <span_id>
```

Check status:

```bash
cargo run -p otell -- status
```

## JSON mode

All query commands support global `--json` output:

```bash
cargo run -p otell -- --json search "error" --since 10m
```

## MCP mode

`otell mcp` reads one JSON request per line from stdin and writes one JSON response per line to stdout.

Supported tools:

- `search`
- `trace`
- `span`
- `traces`
- `metrics`
- `status`

Example:

```bash
printf '{"tool":"status","args":{}}\n' | cargo run -p otell -- mcp
```

## Project layout

- `crates/core` - shared domain types/config/parsing/query types
- `crates/store` - DuckDB schema, inserts, queries, retention
- `crates/ingest` - OTLP decode, gRPC/HTTP ingest servers, write pipeline
- `crates/otell` - CLI, query server/client, output formatting
- `crates/testkit` - deterministic test fixtures

## Testing

Run full test suite:

```bash
cargo test
```

Run a single crate:

```bash
cargo test -p otell-store
```

## Notes

- This is a local debugging utility, not a production observability backend.
- Query API defaults to local-only UDS with `0600` perms where supported.
- OTLP ingest uses TCP for broad SDK compatibility.
