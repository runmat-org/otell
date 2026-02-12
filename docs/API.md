# API

`otell` exposes query APIs over three transports:

- UDS line-delimited JSON (default local transport)
- TCP line-delimited JSON
- HTTP JSON endpoints

It ingests telemetry over OTLP:

- gRPC: `4317` (default)
- HTTP protobuf: `4318` (default)

## Query Protocol (UDS/TCP)

Each request is one JSON line encoded as `ApiRequest`.
Each response is one JSON line encoded as `ApiResponse`.

Request variants:

- `Search(SearchRequest)`
- `Trace(TraceRequest)`
- `Span(SpanRequest)`
- `Traces(TracesRequest)`
- `Metrics(MetricsRequest)`
- `MetricsList(MetricsListRequest)`
- `ResolveHandle(QueryHandle)`
- `Status`

Response variants:

- `Search(SearchResponse)`
- `Trace(TraceResponse)`
- `Span(SpanResponse)`
- `Traces(Vec<TraceListItem>)`
- `Metrics(MetricsResponse)`
- `MetricsList(MetricsListResponse)`
- `Status(StatusResponse)`
- `Error(String)`

## HTTP Query Endpoints

Base address: `--query-http-addr` (default `127.0.0.1:1778`)

- `POST /v1/search` body: `SearchRequest`
- `POST /v1/trace` body: `TraceRequest`
- `GET /v1/trace/{trace_id}` (bounded logs, no root override)
- `POST /v1/span` body: `SpanRequest`
- `POST /v1/traces` body: `TracesRequest`
- `POST /v1/metrics` body: `MetricsRequest`
- `POST /v1/metrics/list` body: `MetricsListRequest`
- `GET /v1/status`

All endpoints return `ApiResponse` JSON.

## MCP (stdio)

Run `otell mcp` and communicate over stdin/stdout.

Supported JSON-RPC methods:

- `initialize`
- `tools/list`
- `tools/call`

Supported tool names:

- `search`
- `trace`
- `span`
- `traces`
- `metrics`
- `metrics.list`
- `status`
- `resolve_handle`
