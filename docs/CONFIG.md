# Configuration

Configuration is loaded from environment variables, then optionally overridden by CLI flags on `otell run`.

## Environment variables

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
