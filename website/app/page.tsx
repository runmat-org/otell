"use client";

import { useMemo, useState } from "react";

type Platform = "macos" | "linux" | "windows";

const AGENTS_MD_CONTENT = `# Development Logging and Observability

The development setup is forwarding logs/traces/metrics to a local OpenTelemetry collector called \`otell\` that is listening on 4317/4318.

To see the logs/traces/metrics, you can use the \`otell\` binary. Run \`otell intro\` to learn how to use the binary to search for logs/traces/metrics.

\`\`\`
otell intro
otell search
otell traces
otell trace
otell span
\`\`\`
`.trim();

const START_OTELL_CONTENT = `
otell run
`.trim();

const SEND_TELEMETRY_CONTENT = `
export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
export OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
`.trim();

const ADVANCED_USAGE_CONTENT = `
export OTELL_FORWARD_OTLP_ENDPOINT=http://remote-collector.example
export OTELL_FORWARD_OTLP_PROTOCOL=http/protobuf
export OTELL_FORWARD_OTLP_COMPRESSION=gzip
export OTELL_FORWARD_OTLP_HEADERS=x-tenant=dev,authorization=Bearer abc123
export OTELL_FORWARD_OTLP_TIMEOUT=10s
`.trim();

const CONFIG_CONTENT = `
#   Linux:   ~/.config/otell/config.toml
#   macOS:   ~/.config/otell/config.toml
#   Windows: %APPDATA%\\otell\\config.toml

otlp_grpc_addr = "127.0.0.1:4317"
otlp_http_addr = "127.0.0.1:4318"

db_path = "/Users/me/.local/share/otell/otell.duckdb"
uds_path = "/tmp/otell.sock"
query_tcp_addr = "127.0.0.1:1777"
query_http_addr = "127.0.0.1:1778"

retention_ttl = "24h"
retention_max_bytes = 2147483648
write_batch_size = 2048
write_flush_ms = 200

forward_otlp_endpoint = "http://remote-collector.example"
forward_otlp_protocol = "grpc" # or "http/protobuf"
forward_otlp_compression = "none" # or "gzip"
forward_otlp_headers = "x-tenant=dev,authorization=Bearer abc123"
forward_otlp_timeout = "10s"
  `.trim();

function detectPlatform(): Platform {
  if (typeof navigator === "undefined") {
    return "macos";
  }
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes("win")) {
    return "windows";
  }
  if (ua.includes("linux")) {
    return "linux";
  }
  return "macos";
}

export default function Home() {
  const [platform, setPlatform] = useState<Platform>(() => detectPlatform());
  const [linkCopied, setLinkCopied] = useState<boolean>(false);
  const [agentMdCopied, setAgentMdCopied] = useState<boolean>(false);
  const [sendTelemetryCopied, setSendTelemetryCopied] = useState<boolean>(false);

  const installCommand = useMemo((): string => {
    if (platform === "windows") {
      return "iwr https://otell.dev/install.ps1 -useb | iex";
    }
    return "curl -fsSL https://otell.dev/install.sh | sh";
  }, [platform]);

  const onInstallCopy = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(installCommand);
      setLinkCopied(true);
      window.setTimeout(() => setLinkCopied(false), 1200);
    } catch {
      setLinkCopied(false);
    }
  };

  const onAgentMdCopy = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(AGENTS_MD_CONTENT);
      setAgentMdCopied(true);
      window.setTimeout(() => setAgentMdCopied(false), 1200);
    } catch {
      setAgentMdCopied(false);
    }
  };

  const onSendTelemetryCopy = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(SEND_TELEMETRY_CONTENT);
      setSendTelemetryCopied(true);
      window.setTimeout(() => setSendTelemetryCopied(false), 1200);
    } catch {
      setSendTelemetryCopied(false);
    }
  };
  const onViewSource = (): void => {
    window.open("https://github.com/runmat-org/otell", "_blank");
  };

  return (
    <main className="container">
      <section className="intro">
        <h1>otell</h1>
        <p>otell is a local OpenTelemetry query tool designed for LLM agents.</p>
      </section>
      <section className="step-panel" aria-label="Installer">
        <h2>Install</h2>
        <div className="selector" role="tablist" aria-label="Operating system">
          <button
            type="button"
            role="tab"
            aria-selected={platform === "macos" || platform === "linux"}
            className={platform === "macos" || platform === "linux" ? "active" : ""}
            onClick={() => setPlatform("linux")}
          >
            Linux / macOS
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={platform === "windows"}
            className={platform === "windows" ? "active" : ""}
            onClick={() => setPlatform("windows")}
          >
            Windows
          </button>
        </div>

        <div className="commandRow">
          <pre>{installCommand}</pre>
          <button type="button" onClick={onInstallCopy} className="copyButton">
            {linkCopied ? "Copied" : "Copy"}
          </button>
        </div>
      </section>

      <section className="step-panel">
        <h2>Start otell</h2>
        <pre>
          {START_OTELL_CONTENT}
        </pre>
      </section>

      <section className="step-panel">
        <h2>Send your logs/traces/metrics to otell</h2>
        <div className="commandRow">
          <pre>{SEND_TELEMETRY_CONTENT}</pre>
          <button type="button" onClick={onSendTelemetryCopy} className="copyButton">
            {sendTelemetryCopied ? "Copied" : "Copy"}
          </button>
        </div>
      </section>

      <section className="step-panel">
        <h2>Teach your model how to use otell</h2>
        <p>
          Add the following to your AGENTS.md:
        </p>
        <pre>
          {AGENTS_MD_CONTENT}
        </pre>
        <div className="commandRow"> <button type="button" onClick={onAgentMdCopy} className="copyButton">
          {agentMdCopied ? "Copied" : "Copy"}
        </button></div>
      </section>

      <section className="step-panel">
        <h2>Advanced usage</h2>

        <p>Tee a copy of your logs/traces/metrics to a human-friendly collector (e.g. Datadog, Dash0, HyperDX, etc.):
        </p>
        <pre>
          {ADVANCED_USAGE_CONTENT}</pre>
        <br />
        <p><a href="https://github.com/runmat-org/otell/blob/main/docs/CONFIG.md">Configure otell</a> via environment variables or config file:</p>
        <pre>
          {CONFIG_CONTENT}
        </pre>
      </section>
      <button type="button" className="viewSourceButton" onClick={onViewSource}>View Source (GitHub)</button>
    </main>
  );
}
