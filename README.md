# otell

`otell` is a local, single-binary OpenTelemetry ingest + query tool for fast debugging loops.

It receives OTLP logs/traces/metrics, stores them in DuckDB, and lets you query deterministically from CLI, HTTP, local sockets, or MCP.

## Why otell

- local-first workflow (no external backend required)
- grep-like log/trace exploration with stable outputs
- machine-friendly JSON for automation and model-driven tooling
- one binary for ingest, query, and onboarding

## Quick start

1) Start `otell`:

```bash
otell run
```

2) Point your app at local OTLP HTTP:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
export OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
```

3) Run onboarding probes:

```bash
otell intro
```

4) Query data:

```bash
otell search "error|timeout" --since 15m --stats
otell traces --since 15m --limit 20
otell trace <trace_id>
```

## Docs

### Model bootstrap 

Paste the contents of `MODEL_QUICKSTART.md` into your AGENTS.md file or equivalent to teach your model how to use otell.

### Reference documentation

- CLI reference: [docs/CLI.md](docs/CLI.md)
- API reference: [docs/API.md](docs/API.md)
- Architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Configuration: [docs/CONFIG.md](docs/CONFIG.md)

## Common commands

- `otell run` start ingest/query services
- `otell intro [--human] [--json]` onboarding probes
- `otell search <pattern>` deterministic log search
- `otell traces`, `otell trace`, `otell span` trace drill-down
- `otell metrics <name>` and `otell metrics list`
- `otell status` storage/health snapshot
- `otell handle <base64>` re-run encoded query handle
- `otell mcp` JSON-RPC MCP server over stdio

## Project layout

- `crates/core` shared models/config/errors/query types
- `crates/store` DuckDB schema, insert/query paths, retention
- `crates/ingest` OTLP gRPC/HTTP receivers + decode pipeline
- `crates/otell` CLI, query servers, MCP, output formatting
- `crates/testkit` test fixtures

## Development

Run tests:

```bash
cargo test
```

Format code:

```bash
cargo fmt
```
