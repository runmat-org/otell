# Configuration

Configuration now has a single load path with clear precedence:

1. built-in defaults
2. local config file (`OTELL_CONFIG` or platform default path)
3. environment variables
4. CLI flags on `otell run` (for bind/path flags only)

This makes it easy to keep a persistent local setup (for example forwarding settings) without exporting env vars in every shell.

## Config file

Default path:

- `${XDG_CONFIG_HOME:-$HOME/.config}/otell/config.toml`

Override path:

- `OTELL_CONFIG=/path/to/config.toml`

If the file does not exist, `otell` continues with defaults/env.

Example:

```toml
db_path = "/Users/me/.local/share/otell/otell.duckdb"
otlp_grpc_addr = "127.0.0.1:4317"
otlp_http_addr = "127.0.0.1:4318"
query_tcp_addr = "127.0.0.1:1777"
query_http_addr = "127.0.0.1:1778"
uds_path = "/tmp/otell.sock"

retention_ttl = "24h"
retention_max_bytes = 2147483648
write_batch_size = 2048
write_flush_ms = 200

forward_otlp_endpoint = "http://127.0.0.1:4317"
forward_otlp_protocol = "grpc" # or "http/protobuf"
forward_otlp_compression = "none" # or "gzip"
forward_otlp_headers = "x-tenant=dev,authorization=Bearer abc123"
forward_otlp_timeout = "10s"
```

## Environment variables

- `OTELL_CONFIG`
  - optional path to config file
  - default config file path: `${XDG_CONFIG_HOME:-$HOME/.config}/otell/config.toml`

- `OTELL_DB_PATH`
  - DuckDB file path
  - default: `${XDG_DATA_HOME:-$HOME/.local/share}/otell/otell.duckdb`

- `OTELL_OTLP_GRPC_ADDR`
  - OTLP gRPC ingest bind address
  - default: `127.0.0.1:4317`

- `OTELL_OTLP_HTTP_ADDR`
  - OTLP HTTP ingest bind address
  - default: `127.0.0.1:4318`

- `OTELL_QUERY_TCP_ADDR`
  - query TCP bind address
  - default: `127.0.0.1:1777`

- `OTELL_QUERY_HTTP_ADDR`
  - query HTTP bind address
  - default: `127.0.0.1:1778`

- `OTELL_QUERY_UDS_PATH`
  - UDS path for query server/client
  - default:
    - `${XDG_RUNTIME_DIR}/otell.sock` when `XDG_RUNTIME_DIR` exists
    - otherwise `${XDG_DATA_HOME:-$HOME/.local/share}/otell/otell.sock`

- `OTELL_RETENTION_TTL`
  - data retention duration
  - default: `24h`
  - format: human durations (`15m`, `2h`, `24h`)

- `OTELL_RETENTION_MAX_BYTES`
  - coarse DB size cap for pruning
  - default: `2147483648` (2 GiB)

- `OTELL_SELF_OBSERVE`
  - controls whether `otell` runtime logs/spans are written back into local store
  - values: `off` (default), `store`, `both`
  - `store`: direct in-process write (no transport)
  - `both`: in-process write + OTLP exporter (if OTEL exporter env is set)

- `OTELL_FORWARD_OTLP_ENDPOINT`
  - optional upstream collector endpoint for forwarding inbound telemetry
  - examples:
    - gRPC: `http://127.0.0.1:4317`
    - HTTP protobuf: `http://127.0.0.1:4318`

- `OTELL_FORWARD_OTLP_PROTOCOL`
  - forwarding transport for inbound telemetry
  - values: `grpc` (default), `http/protobuf`

- `OTELL_FORWARD_OTLP_COMPRESSION`
  - outbound compression for forwarded inbound telemetry
  - values: `none` (default), `gzip`

- `OTELL_FORWARD_OTLP_HEADERS`
  - additional headers/metadata for forwarded inbound telemetry
  - format: comma-separated `key=value` pairs
  - example: `x-tenant=dev,authorization=Bearer abc123`

- `OTELL_FORWARD_OTLP_TIMEOUT`
  - request timeout for forwarded inbound telemetry
  - default: `10s`
  - format: human durations (`500ms`, `5s`, `1m`)

## OTEL exporter env support

`otell` uses OpenTelemetry exporter env conventions for outbound trace export.

Most common:

- `OTEL_EXPORTER_OTLP_ENDPOINT`
- `OTEL_EXPORTER_OTLP_PROTOCOL`
- `OTEL_EXPORTER_OTLP_HEADERS`

When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, `otell` enables outbound trace export for otell's own runtime tracing via `tracing-opentelemetry`.

Inbound telemetry forwarding is controlled separately by `OTELL_FORWARD_OTLP_*`.

## Runtime write settings

Current defaults (internal, not env-tunable in this version):

- `write_batch_size = 2048`
- `write_flush_ms = 200`

## CLI overrides (`otell run`)

These flags override env/default values for that process:

- `--db-path`
- `--otlp-grpc-addr`
- `--otlp-http-addr`
- `--query-tcp-addr`
- `--query-http-addr`
- `--query-uds-path`

## Client-side connection flags

Most query commands accept global flags:

- `--uds <path>` connect via UDS
- `--addr <host:port>` connect via TCP
- `--json` request machine-readable output

If `--uds` is not provided, the client tries `OTELL_QUERY_UDS_PATH` first, then falls back to TCP.
