# Getting Started

This guide walks through a first-time local setup of `otell` and a practical debug workflow.

## 1) Install

macOS/Linux:

```bash
curl -fsSL https://otell.dev/install.sh | sh
```

Windows (PowerShell):

```powershell
iwr https://otell.dev/install.ps1 -useb | iex
```

Confirm install:

```bash
otell --version
```

## 2) Start `otell`

Open terminal A:

```bash
otell run
```

You should see listener addresses for ingest and query services.

## 3) Point your app to local OTLP

In your app environment:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
export OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
```

Then run your app normally so it emits telemetry.

## 4) Verify setup quickly

Open terminal B:

```bash
otell intro
```

This runs a few live probes (`status`, `metrics list`, and an error-signal search).

## 5) Query logs and traces

Search logs:

```bash
otell search "error|timeout" --since 15m --stats
```

List traces:

```bash
otell traces --since 15m --limit 20
```

Inspect a trace:

```bash
otell trace <trace_id>
```

Inspect a span:

```bash
otell span <trace_id> <span_id>
```

## 6) Stream live logs

```bash
otell tail "timeout" --service api --severity WARN
```

This uses a push stream (SSE), not polling.

## 7) Re-run a previous query

Most query commands print a `handle=...` token.

Replay it:

```bash
otell handle <base64-handle>
```

## 8) Common troubleshooting

No results:

- confirm your app is sending OTLP to `127.0.0.1:4318`
- widen your time window (example `--since 24h`)
- check counts:

```bash
otell status
```

`intro` says disconnected:

- ensure `otell run` is still running in another terminal
- if needed, target TCP explicitly:

```bash
otell intro --addr 127.0.0.1:1777
```

## Next docs

- Command reference: `docs/CLI.md`
- API reference: `docs/API.md`
- Configuration: `docs/CONFIG.md`
- Architecture: `docs/ARCHITECTURE.md`
- Development + release: `docs/DEVELOPMENT.md`
