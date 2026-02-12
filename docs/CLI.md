# CLI

`otell` is a single binary for ingest + query.

Global flags:

- `--json` return JSON instead of human output
- `--uds <path>` connect query client over Unix socket
- `--addr <host:port>` connect query client over TCP

## Commands

`otell run`

- Starts OTLP ingest: gRPC + HTTP
- Starts query servers: UDS + TCP + HTTP
- Key flags:
  - `--db-path <path>`
  - `--otlp-grpc-addr <host:port>`
  - `--otlp-http-addr <host:port>`
  - `--query-tcp-addr <host:port>`
  - `--query-http-addr <host:port>`
  - `--query-uds-path <path>`

`otell search <pattern>`

- Grep-like log search with deterministic filtering/sorting.
- Key flags:
  - `--fixed`, `-i/--ignore-case`
  - `--since`, `--until`
  - `--service`, `--trace`, `--span`
  - `--severity <LEVEL>`
  - `--where key=glob` (repeatable)
  - `-C <N|DURATION>` context lines or time-window context (example `-C 20`, `-C 2s`)
  - `--count` return count only
  - `--stats` include grouped stats
  - `--sort ts_asc|ts_desc`
  - `--limit`

`otell traces`

- Lists traces in a window.
- Flags: `--since`, `--until`, `--service`, `--status`, `--sort`, `--limit`

`otell trace <trace_id>`

- Shows trace spans + log context.
- Flags: `--root <span_id>`, `--logs none|bounded|all`

`otell span <trace_id> <span_id>`

- Shows one span with optional related logs.
- Flag: `--logs none|bounded|all`

`otell metrics [<name>|list]`

- `metrics <name>` queries metric points/series.
- `metrics list` lists metric names by occurrence count.
- Flags: `--since`, `--until`, `--service`, `--group-by`, `--agg`, `--limit`

`otell status`

- Returns DB health + counts + oldest/newest timestamps.

`otell handle <base64>`

- Executes an encoded request handle emitted by query commands.

`otell intro`

- LLM-first onboarding via live probes (`status`, `metrics list`, `search count+stats`).
- `--human` prints a more explanatory variant.

`otell mcp`

- MCP-compatible stdio mode.
- Supports JSON-RPC methods: `initialize`, `tools/list`, `tools/call`.

## Typical flow

```bash
# Run the server
otell run

# Onboarding for LLMs
otell intro

# Query the server
otell search "error|timeout" --since 15m --stats
otell traces --since 15m --limit 20
otell trace <trace_id>
otell span <trace_id> <span_id>
```
