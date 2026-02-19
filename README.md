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

3) Add the following to your AGENTS.md file or equivalent to teach your model how to use otell.

```markdown
# Development Logging and Observability

The development setup is forwarding logs/traces/metrics to a local OpenTelemetry Collector called `otell` that is listening on 4317/4318.

To see the logs/traces/metrics, you can use the `otell` binary. Run `otell intro` to learn how to use the binary to search for logs/traces/metrics.

\`\`\`
otell intro
otell search
otell traces
otell trace
otell span
\`\`\`
```

4) (Optional) query data yourself:

```bash
otell search "error|timeout" --since 15m --stats
otell traces --since 15m --limit 20
otell trace <trace_id>
```