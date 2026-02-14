# API

This document defines the public API surfaces for `otell`.

## Scope and stability

`otell` currently exposes four API surfaces:

- Query over UDS line-delimited JSON
- Query over TCP line-delimited JSON
- Query over HTTP JSON
- MCP over stdio (JSON-RPC)

Ingest is OpenTelemetry Protocol (OTLP) over:

- gRPC (`4317` default)
- HTTP protobuf (`4318` default)

Current compatibility target: stable command/query semantics for local debugging workflows. New fields may be added over time; callers should ignore unknown fields.

## Trust model and networking

- Default posture is local-only development usage.
- UDS query socket is permission-restricted on Unix (`0600`).
- Treat TCP/HTTP bindings as trusted-network interfaces.
- No built-in authentication/authorization is implemented.

## Determinism guarantees

The query API is designed to be deterministic:

- Explicit sort and limit behavior
- Stable filtering semantics
- Bounded context policies where applicable
- No relevance ranking or heuristic ordering

## Shared query envelope (UDS/TCP)

UDS/TCP query protocol is one JSON request line -> one JSON response line.

Requests use `ApiRequest` variants:

- `Search(SearchRequest)`
- `Trace(TraceRequest)`
- `Span(SpanRequest)`
- `Traces(TracesRequest)`
- `Metrics(MetricsRequest)`
- `MetricsList(MetricsListRequest)`
- `ResolveHandle(QueryHandle)`
- `Status`

Responses use `ApiResponse` variants:

- `Search(SearchResponse)`
- `Trace(TraceResponse)`
- `Span(SpanResponse)`
- `Traces(Vec<TraceListItem>)`
- `Metrics(MetricsResponse)`
- `MetricsList(MetricsListResponse)`
- `Status(StatusResponse)`
- `Error(String)`

## Key request semantics

### `SearchRequest`

Important fields:

- `pattern`: regex by default
- `fixed`: literal substring mode
- `ignore_case`: case-insensitive matching
- `window`: `since` / `until`
- `service`, `trace_id`, `span_id`, `severity_gte`
- `attr_filters`: key/glob filters
- `sort`: `TsAsc` / `TsDesc`
- `limit`
- context controls:
  - `context_lines`
  - `context_seconds`
- `count_only`
- `include_stats`

### `TraceRequest` / `SpanRequest`

- `logs` policy: `None`, `Bounded`, `All`
- bounded mode uses fixed limits and reports truncation metadata

### `MetricsRequest`

- `name` selects metric stream
- optional `service`
- optional `group_by` and aggregation (`avg`, `count`, `min`, `max`, `p50`, `p95`, `p99`)

### `ResolveHandle`

- Handles are encoded request payloads emitted by CLI query commands.
- Resolving a handle replays the original request.

## HTTP query API

Base address: `--query-http-addr` (default `127.0.0.1:1778`).

Endpoints:

- `POST /v1/search` body: `SearchRequest`
- `POST /v1/trace` body: `TraceRequest`
- `GET /v1/trace/{trace_id}` (bounded logs, no root override)
- `POST /v1/span` body: `SpanRequest`
- `POST /v1/traces` body: `TracesRequest`
- `POST /v1/metrics` body: `MetricsRequest`
- `POST /v1/metrics/list` body: `MetricsListRequest`
- `GET /v1/status`
- `GET /v1/tail` SSE stream

All HTTP query endpoints return `ApiResponse` JSON, except `/v1/tail`.

### Example: search

Request:

```http
POST /v1/search
Content-Type: application/json
```

```json
{
  "pattern": "timeout",
  "fixed": false,
  "ignore_case": false,
  "service": "api",
  "trace_id": null,
  "span_id": null,
  "severity_gte": null,
  "attr_filters": [],
  "window": { "since": null, "until": null },
  "sort": "TsAsc",
  "limit": 100,
  "context_lines": 0,
  "context_seconds": null,
  "count_only": false,
  "include_stats": true
}
```

Response:

```json
{
  "Search": {
    "total_matches": 1,
    "returned": 1,
    "records": [
      {
        "ts": "2026-02-12T20:22:45.102Z",
        "service": "api",
        "severity": 17,
        "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
        "span_id": "00f067aa0ba902b7",
        "body": "context deadline exceeded",
        "attrs_json": "{\"peer\":\"redis:6379\"}",
        "attrs_text": "peer=redis:6379"
      }
    ],
    "stats": {
      "by_service": [["api", 1]],
      "by_severity": [["ERROR", 1]]
    }
  }
}
```

### Example: error response

```json
{
  "Error": "invalid regex pattern: ..."
}
```

## Streaming: `/v1/tail`

`GET /v1/tail` uses Server-Sent Events (SSE).

Supported query params:

- `pattern`
- `fixed`
- `ignore_case`
- `service`
- `trace_id`
- `span_id`
- `severity`

Event shape:

- `data:` frame contains serialized `LogRecord` JSON.

Example frame:

```text
data: {"ts":"2026-02-12T20:25:02.004Z","service":"api","severity":17,"trace_id":null,"span_id":null,"body":"context deadline exceeded","attrs_json":"{}","attrs_text":""}

```

## OTLP ingest

Ingest accepts OTLP from applications and SDKs:

- gRPC services for logs/traces/metrics
- HTTP protobuf endpoints:
  - `POST /v1/logs`
  - `POST /v1/traces`
  - `POST /v1/metrics`

Ingest behavior:

- decode OTLP payloads to internal records
- batch and commit to DuckDB
- optionally forward inbound payloads to upstream collector (`OTELL_FORWARD_OTLP_*`)

## MCP (stdio JSON-RPC)

Run:

```bash
otell mcp
```

Supported methods:

- `initialize`
- `tools/list`
- `tools/call`

Tool names:

- `search`
- `trace`
- `span`
- `traces`
- `metrics`
- `metrics.list`
- `status`
- `resolve_handle`

`tools/call` maps directly to `ApiRequest` equivalents.

## Runtime telemetry vs inbound forwarding

These are separate paths:

- `OTEL_EXPORTER_OTLP_*`: exports **otell runtime tracing** (internal spans/events)
- `OTELL_SELF_OBSERVE`: writes **otell runtime logs/spans** into local DuckDB
- `OTELL_FORWARD_OTLP_*`: forwards **inbound application telemetry** to upstream collector

This separation lets you choose local-only, self-observed, upstream-exported, or tee-forwarded modes independently.
