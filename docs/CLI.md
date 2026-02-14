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

Example:

```bash
otell run
```

Example output:

```text
INFO otell starting
INFO ingest gRPC listening on 127.0.0.1:4317
INFO ingest HTTP listening on 127.0.0.1:4318
INFO query UDS listening on /tmp/otell.sock
INFO query TCP listening on 127.0.0.1:1777
INFO query HTTP listening on 127.0.0.1:1778
```

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

Example:

```bash
otell search "timeout" --since 15m --stats
```

Example output:

```text
2026-02-12T20:22:45.102Z api ERROR trace=4bf92f3577b34da6a3ce929d0e0e4736 span=00f067aa0ba902b7 | context deadline exceeded peer=redis:6379
-- 1 matches (1 returned) --
stats.by_service=[("api", 1)]
stats.by_severity=[("ERROR", 1)]
handle=eyJTZWFyY2giOnsicGF0dGVybiI6InRpbWVvdXQiLC4uLn19
```

`otell traces`

- Lists traces in a window.
- Flags: `--since`, `--until`, `--service`, `--status`, `--sort`, `--limit`

Example:

```bash
otell traces --since 15m --limit 2
```

Example output:

```text
trace=4bf92f3577b34da6a3ce929d0e0e4736 duration=1800ms spans=3 status=ERROR root="GET /v1/orders"
trace=5af7183f9cbe40f598b7ebf9f9830cbf duration=230ms spans=2 status=OK root="GET /healthz"
-- 2 traces --
handle=eyJUcmFjZXMiOnsibGltaXQiOjIsLi4ufX0=
```

`otell trace <trace_id>`

- Shows trace spans + log context.
- Flags: `--root <span_id>`, `--logs none|bounded|all`

Example:

```bash
otell trace 4bf92f3577b34da6a3ce929d0e0e4736
```

Example output:

```text
TRACE 4bf92f3577b34da6a3ce929d0e0e4736 duration=1800ms spans=3 errors=1
api GET /v1/orders (1800ms) ERROR
  api cache.get redis (700ms) ERROR
logs=bounded limit=50 truncated=false
2026-02-12T20:22:45.102Z api ERROR | context deadline exceeded
handle=eyJUcmFjZSI6eyJ0cmFjZV9pZCI6IjRiZjkyLi4uIn19
```

`otell span <trace_id> <span_id>`

- Shows one span with optional related logs.
- Flag: `--logs none|bounded|all`

Example:

```bash
otell span 4bf92f3577b34da6a3ce929d0e0e4736 00f067aa0ba902b7
```

Example output:

```text
SPAN 00f067aa0ba902b7 service=api name=cache.get redis status=ERROR duration=700ms
attrs={"peer":"redis:6379"}
events=[]
logs=bounded limit=30 truncated=false
2026-02-12T20:22:45.102Z ERROR | context deadline exceeded
handle=eyJTcGFuIjp7InRyYWNlX2lkIjoiNGJmOTIuLi4ifX0=
```

`otell metrics [<name>|list]`

- `metrics <name>` queries metric points/series.
- `metrics list` lists metric names by occurrence count.
- Flags: `--since`, `--until`, `--service`, `--group-by`, `--agg`, `--limit`

Examples:

```bash
otell metrics list --since 15m
```

```text
name=http.server.duration count=42
name=process.runtime.nodejs.eventloop.utilization count=9
-- 2 metric names --
handle=eyJNZXRyaWNzTGlzdCI6eyJsaW1pdCI6NTAsLi4ufX0=
```

```bash
otell metrics http.server.duration --group-by service --agg p95
```

```text
points=42
group=api value=182.4
-- 1 series (42 points) --
handle=eyJNZXRyaWNzIjp7Im5hbWUiOiJodHRwLnNlcnZlci5kdXJhdGlvbiIsLi4ufX0=
```

`otell tail [pattern]`

- Streams matching logs in real time using server push (SSE, no polling).
- Flags: `--fixed`, `-i/--ignore-case`, `--service`, `--trace`, `--span`, `--severity`, `--http-addr`

Example:

```bash
otell tail timeout --service api --severity WARN
```

Example output:

```text
2026-02-12T20:25:01.331Z api WARN | retrying attempt=2
2026-02-12T20:25:02.004Z api ERROR | context deadline exceeded
```

`otell status`

- Returns DB health + counts + oldest/newest timestamps.

Example:

```bash
otell status
```

Example output:

```text
db_path=/Users/me/.local/share/otell/otell.duckdb
db_size_bytes=786432
logs=312 spans=122 metrics=88
oldest=2026-02-12T19:31:02.481Z
newest=2026-02-12T20:22:45.102Z
handle=eyJTdGF0dXMiOm51bGx9
```

`otell handle <base64>`

- Executes an encoded request handle emitted by query commands.

Example:

```bash
otell handle eyJTdGF0dXMiOm51bGx9
```

Example output:

```text
db_path=/Users/me/.local/share/otell/otell.duckdb
db_size_bytes=786432
logs=312 spans=122 metrics=88
oldest=2026-02-12T19:31:02.481Z
newest=2026-02-12T20:22:45.102Z
handle=eyJTdGF0dXMiOm51bGx9
```

`otell intro`

- LLM-first onboarding via live probes (`status`, `metrics list`, `search count+stats`).
- `--human` prints a more explanatory variant.

Example:

```bash
otell intro
```

Example output:

```text
INTRO mode=llm connected=true
probe=status
db_path=/Users/me/.local/share/otell/otell.duckdb
db_size_bytes=786432
logs=312 spans=122 metrics=88
probe=metrics_list
name=http.server.duration count=42
-- 1 metric names --
probe=search_count_stats pattern=error|timeout
-- 7 matches (0 returned) --
stats.by_service=[("api", 7)]
next=otell traces --since 15m --limit 20
next=otell trace <trace_id>
next=otell span <trace_id> <span_id>
next=otell handle <base64>
```

`otell mcp`

- MCP-compatible stdio mode.
- Supports JSON-RPC methods: `initialize`, `tools/list`, `tools/call`.

Example:

```bash
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n' | otell mcp
```

Example output:

```json
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"0.1.0","serverInfo":{"name":"otell","version":"0.1.0"},"capabilities":{"tools":{"listChanged":false}}}}
```

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
