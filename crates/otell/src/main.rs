mod client;
mod output;
mod protocol;
mod query_server;
mod telemetry;

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use base64::Engine;
use clap::{Parser, Subcommand};
use otell_core::config::Config;
use otell_core::filter::{AttrFilter, Severity, SortOrder, TimeWindow};
use otell_core::query::{
    LogContextMode, MetricsListRequest, MetricsRequest, QueryHandle, SearchRequest, SpanRequest,
    TraceRequest, TracesRequest,
};
use otell_core::time::{parse_duration_str, parse_time_or_relative};
use otell_ingest::forward::{ForwardConfig, ForwardProtocol};
use otell_ingest::pipeline::PipelineConfig;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::client::QueryClient;
use crate::output::{
    print_metrics_human, print_metrics_list_human, print_search_human, print_span_human,
    print_status_human, print_trace_human, print_traces_human,
};
use crate::protocol::{ApiRequest, ApiResponse};
use crate::telemetry::{
    SelfObserveMode, TelemetryConfig, init_cli_tracing, init_run_tracing, shutdown_tracing,
};

#[derive(Parser, Debug)]
#[command(name = "otell")]
#[command(about = "Local OTEL ingest and query utility")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, global = true)]
    json: bool,

    #[arg(long, global = true)]
    uds: Option<PathBuf>,

    #[arg(long, global = true)]
    addr: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Run ingest and query servers")]
    Run {
        #[arg(long)]
        db_path: Option<PathBuf>,
        #[arg(long)]
        otlp_grpc_addr: Option<String>,
        #[arg(long)]
        otlp_http_addr: Option<String>,
        #[arg(long)]
        query_tcp_addr: Option<String>,
        #[arg(long)]
        query_http_addr: Option<String>,
        #[arg(long)]
        query_uds_path: Option<PathBuf>,
    },
    #[command(about = "Search logs with deterministic filters")]
    Search {
        pattern: String,
        #[arg(long)]
        fixed: bool,
        #[arg(short = 'i', long)]
        ignore_case: bool,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
        #[arg(long)]
        service: Option<String>,
        #[arg(long)]
        trace: Option<String>,
        #[arg(long)]
        span: Option<String>,
        #[arg(long)]
        severity: Option<String>,
        #[arg(long = "where")]
        where_filters: Vec<String>,
        #[arg(short = 'C', help = "Context lines (e.g. 20) or time (e.g. 2s)")]
        context: Option<String>,
        #[arg(long, help = "Only return total match count")]
        count: bool,
        #[arg(long, help = "Include grouped stats in response")]
        stats: bool,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value = "ts_asc")]
        sort: String,
    },
    #[command(about = "Inspect a trace and related logs")]
    Trace {
        trace_id: String,
        #[arg(long)]
        root: Option<String>,
        #[arg(long, default_value = "bounded")]
        logs: String,
    },
    #[command(about = "Inspect a specific span")]
    Span {
        trace_id: String,
        span_id: String,
        #[arg(long, default_value = "bounded")]
        logs: String,
    },
    #[command(about = "List traces")]
    Traces {
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
        #[arg(long)]
        service: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value = "duration_desc")]
        sort: String,
    },
    #[command(about = "Query metric points or list metric names")]
    Metrics {
        name: Option<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
        #[arg(long)]
        service: Option<String>,
        #[arg(long)]
        group_by: Option<String>,
        #[arg(long)]
        agg: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    #[command(about = "Stream matching logs in real time")]
    Tail {
        pattern: Option<String>,
        #[arg(long)]
        fixed: bool,
        #[arg(short = 'i', long)]
        ignore_case: bool,
        #[arg(long)]
        service: Option<String>,
        #[arg(long)]
        trace: Option<String>,
        #[arg(long)]
        span: Option<String>,
        #[arg(long)]
        severity: Option<String>,
        #[arg(long)]
        http_addr: Option<String>,
    },
    Status,
    #[command(about = "Execute a previously emitted handle")]
    Handle {
        handle: String,
    },
    #[command(about = "Learn otell quickly via live probes")]
    Intro {
        #[arg(long, help = "Human-friendly explanatory output")]
        human: bool,
    },
    Mcp,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            db_path,
            otlp_grpc_addr,
            otlp_http_addr,
            query_tcp_addr,
            query_http_addr,
            query_uds_path,
        } => {
            let telemetry_cfg = TelemetryConfig {
                self_observe: SelfObserveMode::from_env(),
            };
            run_server(
                db_path,
                otlp_grpc_addr,
                otlp_http_addr,
                query_tcp_addr,
                query_http_addr,
                query_uds_path,
                telemetry_cfg,
            )
            .await
        }
        Commands::Search {
            pattern,
            fixed,
            ignore_case,
            since,
            until,
            service,
            trace,
            span,
            severity,
            where_filters,
            context,
            count,
            stats,
            limit,
            sort,
        } => {
            init_cli_tracing();
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let (context_lines, context_seconds) = parse_context(context)?;
            let req = SearchRequest {
                pattern: Some(pattern),
                fixed,
                ignore_case,
                service,
                trace_id: trace,
                span_id: span,
                severity_gte: severity.map(|s| Severity::from_str(&s)).transpose()?,
                attr_filters: where_filters
                    .into_iter()
                    .map(|f| AttrFilter::parse(&f))
                    .collect::<otell_core::Result<Vec<_>>>()?,
                window: parse_window(since, until)?,
                sort: parse_sort(&sort),
                limit,
                context_lines,
                context_seconds,
                count_only: count,
                include_stats: stats,
            };
            let api_req = ApiRequest::Search(req);
            let handle = encode_handle(&api_req)?;
            let response = client.request(api_req).await?;
            print_response(response, cli.json)?;
            if !cli.json {
                println!("handle={handle}");
            }
            Ok(())
        }
        Commands::Trace {
            trace_id,
            root,
            logs,
        } => {
            init_cli_tracing();
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let req = TraceRequest {
                trace_id,
                root_span_id: root,
                logs: parse_logs_mode(&logs)?,
            };
            let api_req = ApiRequest::Trace(req);
            let handle = encode_handle(&api_req)?;
            let response = client.request(api_req).await?;
            print_response(response, cli.json)?;
            if !cli.json {
                println!("handle={handle}");
            }
            Ok(())
        }
        Commands::Span {
            trace_id,
            span_id,
            logs,
        } => {
            init_cli_tracing();
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let req = SpanRequest {
                trace_id,
                span_id,
                logs: parse_logs_mode(&logs)?,
            };
            let api_req = ApiRequest::Span(req);
            let handle = encode_handle(&api_req)?;
            let response = client.request(api_req).await?;
            print_response(response, cli.json)?;
            if !cli.json {
                println!("handle={handle}");
            }
            Ok(())
        }
        Commands::Traces {
            since,
            until,
            service,
            status,
            limit,
            sort,
        } => {
            init_cli_tracing();
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let req = TracesRequest {
                service,
                status,
                window: parse_window(since, until)?,
                sort: parse_sort(&sort),
                limit,
            };
            let api_req = ApiRequest::Traces(req);
            let handle = encode_handle(&api_req)?;
            let response = client.request(api_req).await?;
            print_response(response, cli.json)?;
            if !cli.json {
                println!("handle={handle}");
            }
            Ok(())
        }
        Commands::Metrics {
            name,
            since,
            until,
            service,
            group_by,
            agg,
            limit,
        } => {
            init_cli_tracing();
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let api_req = if matches!(name.as_deref(), None | Some("list")) {
                ApiRequest::MetricsList(MetricsListRequest {
                    service,
                    window: parse_window(since, until)?,
                    limit,
                })
            } else {
                ApiRequest::Metrics(MetricsRequest {
                    name: name.unwrap_or_else(|| "list".to_string()),
                    service,
                    window: parse_window(since, until)?,
                    group_by,
                    agg,
                    limit,
                })
            };
            let handle = encode_handle(&api_req)?;
            let response = client.request(api_req).await?;
            print_response(response, cli.json)?;
            if !cli.json {
                println!("handle={handle}");
            }
            Ok(())
        }
        Commands::Tail {
            pattern,
            fixed,
            ignore_case,
            service,
            trace,
            span,
            severity,
            http_addr,
        } => {
            init_cli_tracing();
            run_tail(TailQueryParams {
                pattern,
                fixed,
                ignore_case,
                service,
                trace_id: trace,
                span_id: span,
                severity,
                addr: http_addr
                    .or(cli.addr)
                    .or_else(|| std::env::var("OTELL_QUERY_HTTP_ADDR").ok())
                    .unwrap_or_else(|| "127.0.0.1:1778".to_string()),
            })
            .await
        }
        Commands::Status => {
            init_cli_tracing();
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let api_req = ApiRequest::Status;
            let handle = encode_handle(&api_req)?;
            let response = client.request(api_req).await?;
            print_response(response, cli.json)?;
            if !cli.json {
                println!("handle={handle}");
            }
            Ok(())
        }
        Commands::Handle { handle } => {
            init_cli_tracing();
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let req = decode_handle(&handle)?;
            let response = client.request(req).await?;
            print_response(response, cli.json)?;
            Ok(())
        }
        Commands::Intro { human } => {
            init_cli_tracing();
            run_intro(cli.uds, cli.addr, cli.json, human).await
        }
        Commands::Mcp => {
            init_cli_tracing();
            run_mcp(cli.uds, cli.addr).await
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct TailQueryParams {
    pattern: Option<String>,
    fixed: bool,
    ignore_case: bool,
    service: Option<String>,
    trace_id: Option<String>,
    span_id: Option<String>,
    severity: Option<String>,
    #[serde(skip_serializing)]
    addr: String,
}

async fn run_tail(params: TailQueryParams) -> anyhow::Result<()> {
    let url = format!("http://{}/v1/tail", params.addr);
    let client = reqwest::Client::new();
    let mut response = client
        .get(url)
        .query(&params)
        .send()
        .await
        .context("open tail stream")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "tail stream request failed with status {}",
            response.status()
        );
    }

    let mut buffer = String::new();
    while let Some(chunk) = response.chunk().await.context("read tail stream chunk")? {
        let text = std::str::from_utf8(&chunk).context("tail stream contained invalid utf8")?;
        buffer.push_str(text);

        while let Some(frame_end) = buffer.find("\n\n") {
            let frame = buffer[..frame_end].to_string();
            buffer.drain(..frame_end + 2);

            for line in frame.lines() {
                if let Some(data) = line.strip_prefix("data: ")
                    && let Ok(record) =
                        serde_json::from_str::<otell_core::model::log::LogRecord>(data)
                {
                    print_tail_record(&record);
                }
            }
        }
    }

    Ok(())
}

fn print_tail_record(record: &otell_core::model::log::LogRecord) {
    use owo_colors::OwoColorize;

    let sev = match record.severity {
        1..=4 => "TRACE".blue().to_string(),
        5..=8 => "DEBUG".bright_black().to_string(),
        9..=12 => "INFO".green().to_string(),
        13..=16 => "WARN".yellow().to_string(),
        17..=20 => "ERROR".red().to_string(),
        _ => "FATAL".magenta().to_string(),
    };

    println!(
        "{} {} {} | {}",
        record.ts.to_rfc3339(),
        record.service.cyan(),
        sev,
        record.body
    );
}

async fn run_intro(
    uds: Option<PathBuf>,
    addr: Option<String>,
    json: bool,
    human: bool,
) -> anyhow::Result<()> {
    let cfg = otell_core::config::Config::from_env().unwrap_or_default();
    let intro_commands = vec![
        "otell run",
        "otell intro",
        "otell search \"error|timeout\" --since 15m --stats",
        "otell traces --since 15m --limit 20",
        "otell trace <trace_id>",
        "otell span <trace_id> <span_id>",
        "otell tail timeout --service api --severity WARN",
        "otell handle <base64>",
    ];

    let (mut client_opt, connect_error): (Option<QueryClient>, Option<String>) =
        match connect_with_retry(uds, addr).await {
            Ok(c) => (Some(c), None),
            Err(err) => (None, Some(err.to_string())),
        };

    let connected = client_opt.is_some();
    let mut status: Option<ApiResponse> = None;
    let mut metrics: Option<ApiResponse> = None;
    let mut search: Option<ApiResponse> = None;

    if let Some(client) = client_opt.as_mut() {
        status = client.request(ApiRequest::Status).await.ok();
        metrics = client
            .request(ApiRequest::MetricsList(MetricsListRequest {
                service: None,
                window: TimeWindow::all(),
                limit: 5,
            }))
            .await
            .ok();
        search = client
            .request(ApiRequest::Search(SearchRequest {
                pattern: Some("error|timeout".to_string()),
                include_stats: true,
                count_only: true,
                limit: 100,
                ..SearchRequest::default()
            }))
            .await
            .ok();
    }

    if json {
        let payload = serde_json::json!({
            "mode": if human {"human"} else {"llm"},
            "what_is_otell": "local OpenTelemetry ingest + query utility for logs, traces, and metrics",
            "connected": connected,
            "endpoints": {
                "ingest_grpc": cfg.otlp_grpc_addr,
                "ingest_http": cfg.otlp_http_addr,
                "query_uds": cfg.uds_path,
                "query_tcp": cfg.query_tcp_addr,
                "query_http": cfg.query_http_addr,
            },
            "instance_state": {
                "running": connected,
                "connect_error": connect_error,
            },
            "workflow": [
                "search logs for signal",
                "list traces in window",
                "inspect one trace",
                "inspect one span",
                "tail live logs",
                "reuse handles in agent loops"
            ],
            "probes": {
                "status": status,
                "metrics_list": metrics,
                "search_count_stats": search,
            },
            "next_commands": intro_commands,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let markdown = render_intro_markdown(IntroDocInput {
        connected,
        connect_error,
        cfg: &cfg,
        intro_commands: &intro_commands,
        status: status.as_ref(),
        metrics: metrics.as_ref(),
        search: search.as_ref(),
        human,
    })?;
    println!("{markdown}");

    Ok(())
}

struct IntroDocInput<'a> {
    connected: bool,
    connect_error: Option<String>,
    cfg: &'a otell_core::config::Config,
    intro_commands: &'a [&'a str],
    status: Option<&'a ApiResponse>,
    metrics: Option<&'a ApiResponse>,
    search: Option<&'a ApiResponse>,
    human: bool,
}

fn render_intro_markdown(input: IntroDocInput<'_>) -> anyhow::Result<String> {
    let audience = if input.human { "human" } else { "llm" };
    let mut out = String::new();

    out.push_str("# otell onboarding\n\n");
    out.push_str(&format!("_Audience mode: {audience}_\n\n"));
    out.push_str("`otell` is a local OpenTelemetry ingest and query utility for logs, traces, and metrics. This onboarding note is designed to be read directly by a coding model or developer to quickly establish what `otell` is, what instance is available right now, and which commands to run next.\n\n");

    out.push_str("## instance state\n\n");
    out.push_str(&format!(
        "- connected to running `otell run`: `{}`\n",
        input.connected
    ));
    if let Some(err) = input.connect_error {
        out.push_str(&format!("- connection error: `{err}`\n"));
        out.push_str("- action: start `otell run`, then rerun `otell intro`\n");
    }
    out.push('\n');

    out.push_str("## endpoints\n\n");
    out.push_str(&format!("- ingest gRPC: `{}`\n", input.cfg.otlp_grpc_addr));
    out.push_str(&format!("- ingest HTTP: `{}`\n", input.cfg.otlp_http_addr));
    out.push_str(&format!(
        "- query UDS: `{}`\n",
        input.cfg.uds_path.display()
    ));
    out.push_str(&format!("- query TCP: `{}`\n", input.cfg.query_tcp_addr));
    out.push_str(&format!(
        "- query HTTP: `{}`\n\n",
        input.cfg.query_http_addr
    ));

    out.push_str("## recommended workflow\n\n");
    out.push_str("1. Search logs for immediate failure signals (`error`, `timeout`) and inspect aggregate stats.\n");
    out.push_str("2. List traces in a recent window and pick one relevant trace id.\n");
    out.push_str("3. Inspect the trace structure, then zoom into one problematic span.\n");
    out.push_str("4. Use `tail` for real-time follow-up while reproducing the issue.\n");
    out.push_str("5. Reuse emitted handles to replay exact queries in an agent loop.\n\n");

    out.push_str("## next commands\n\n");
    for command in input.intro_commands {
        out.push_str(&format!("- `{command}`\n"));
    }
    out.push('\n');

    if let Some(status) = input.status {
        out.push_str("## live probe: status\n\n```json\n");
        out.push_str(&serde_json::to_string_pretty(status)?);
        out.push_str("\n```\n\n");
    }
    if let Some(metrics) = input.metrics {
        out.push_str("## live probe: metrics list\n\n```json\n");
        out.push_str(&serde_json::to_string_pretty(metrics)?);
        out.push_str("\n```\n\n");
    }
    if let Some(search) = input.search {
        out.push_str("## live probe: search count + stats\n\n```json\n");
        out.push_str(&serde_json::to_string_pretty(search)?);
        out.push_str("\n```\n\n");
    }

    out.push_str("## machine-use notes\n\n");
    out.push_str("- Add `--json` to any query command for structured output.\n");
    out.push_str(
        "- Most query commands print `handle=...`; use `otell handle <base64>` to replay.\n",
    );
    out.push_str("- For local troubleshooting, run `otell status` and then `otell intro` again after state changes.\n");

    Ok(out)
}

async fn connect_with_retry(
    uds: Option<PathBuf>,
    addr: Option<String>,
) -> anyhow::Result<QueryClient> {
    let mut last_err: Option<anyhow::Error> = None;
    for _ in 0..30 {
        match QueryClient::connect(uds.clone(), addr.clone()).await {
            Ok(client) => return Ok(client),
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("failed to connect to otell query server")))
}

async fn run_mcp(uds: Option<PathBuf>, addr: Option<String>) -> anyhow::Result<()> {
    #[derive(serde::Deserialize)]
    struct McpReq {
        id: Option<serde_json::Value>,
        method: Option<String>,
        params: Option<serde_json::Value>,
    }

    fn mcp_ok(id: Option<serde_json::Value>, result: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"jsonrpc":"2.0","id":id,"result":result})
    }

    fn mcp_err(id: Option<serde_json::Value>, message: String) -> serde_json::Value {
        serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"message":message}})
    }

    let mut client: Option<QueryClient> = None;
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        let input: Result<McpReq, _> = serde_json::from_str(&line);
        let input = match input {
            Ok(v) => v,
            Err(e) => {
                println!("{}", serde_json::to_string(&mcp_err(None, e.to_string()))?);
                continue;
            }
        };

        if matches!(input.method.as_deref(), Some("initialize")) {
            let result = serde_json::json!({
                "protocolVersion": "0.1.0",
                "serverInfo": {"name": "otell", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {
                    "tools": {"listChanged": false}
                }
            });
            println!("{}", serde_json::to_string(&mcp_ok(input.id, result))?);
            continue;
        }

        if matches!(input.method.as_deref(), Some("tools/list")) {
            let result = serde_json::json!({"tools": [
                {"name":"search"},
                {"name":"trace"},
                {"name":"span"},
                {"name":"traces"},
                {"name":"metrics"},
                {"name":"metrics.list"},
                {"name":"status"},
                {"name":"resolve_handle"}
            ]});
            println!("{}", serde_json::to_string(&mcp_ok(input.id, result))?);
            continue;
        }

        if !matches!(input.method.as_deref(), Some("tools/call")) {
            println!(
                "{}",
                serde_json::to_string(&mcp_err(
                    input.id,
                    "unsupported method (expected initialize, tools/list, tools/call)".to_string()
                ))?
            );
            continue;
        }

        let tool_name = input
            .params
            .as_ref()
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());
        let Some(tool_name) = tool_name else {
            println!(
                "{}",
                serde_json::to_string(&mcp_err(input.id, "missing tool name".to_string()))?
            );
            continue;
        };

        let method_args = input
            .params
            .as_ref()
            .and_then(|p| p.get("arguments"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let request = match tool_name.as_str() {
            "search" => {
                serde_json::from_value::<SearchRequest>(method_args).map(ApiRequest::Search)
            }
            "trace" => serde_json::from_value::<TraceRequest>(method_args).map(ApiRequest::Trace),
            "span" => serde_json::from_value::<SpanRequest>(method_args).map(ApiRequest::Span),
            "traces" => {
                serde_json::from_value::<TracesRequest>(method_args).map(ApiRequest::Traces)
            }
            "metrics" => {
                serde_json::from_value::<MetricsRequest>(method_args).map(ApiRequest::Metrics)
            }
            "metrics.list" => serde_json::from_value::<MetricsListRequest>(method_args)
                .map(ApiRequest::MetricsList),
            "resolve_handle" => {
                serde_json::from_value::<QueryHandle>(method_args).map(ApiRequest::ResolveHandle)
            }
            "status" => Ok(ApiRequest::Status),
            _ => {
                println!(
                    "{}",
                    serde_json::to_string(&mcp_err(input.id, "unknown mcp tool".to_string()))?
                );
                continue;
            }
        };

        let response = match request {
            Ok(req) => {
                if client.is_none() {
                    client = Some(QueryClient::connect(uds.clone(), addr.clone()).await?);
                }
                client
                    .as_mut()
                    .expect("client initialized")
                    .request(req)
                    .await
                    .unwrap_or_else(|e| ApiResponse::Error(e.to_string()))
            }
            Err(e) => ApiResponse::Error(format!("invalid tool arguments: {e}")),
        };

        println!(
            "{}",
            serde_json::to_string(&mcp_ok(input.id, serde_json::to_value(response)?))?
        );
    }

    Ok(())
}

async fn run_server(
    db_path: Option<PathBuf>,
    otlp_grpc_addr: Option<String>,
    otlp_http_addr: Option<String>,
    query_tcp_addr: Option<String>,
    query_http_addr: Option<String>,
    query_uds_path: Option<PathBuf>,
    telemetry_cfg: TelemetryConfig,
) -> anyhow::Result<()> {
    let mut cfg = Config::from_env().context("load config from env")?;
    if let Some(v) = db_path {
        cfg.db_path = v;
    }
    if let Some(v) = otlp_grpc_addr {
        cfg.otlp_grpc_addr = v;
    }
    if let Some(v) = otlp_http_addr {
        cfg.otlp_http_addr = v;
    }
    if let Some(v) = query_tcp_addr {
        cfg.query_tcp_addr = v;
    }
    if let Some(v) = query_http_addr {
        cfg.query_http_addr = v;
    }
    if let Some(v) = query_uds_path {
        cfg.uds_path = v;
    }

    let store = otell_store::Store::open(&cfg.db_path)?;
    init_run_tracing(telemetry_cfg, Some(store.clone()));

    eprintln!("otell run");
    eprintln!("  db: {}", cfg.db_path.display());
    eprintln!("  ingest grpc: {}", cfg.otlp_grpc_addr);
    eprintln!("  ingest http: {}", cfg.otlp_http_addr);
    eprintln!("  query uds: {}", cfg.uds_path.display());
    eprintln!("  query tcp: {}", cfg.query_tcp_addr);
    eprintln!("  query http: {}", cfg.query_http_addr);
    eprintln!("  tip: run `otell intro` in another shell");

    let grpc_addr = cfg.otlp_grpc_addr.parse()?;
    let http_addr = cfg.otlp_http_addr.parse()?;

    let ingest_task = tokio::spawn(otell_ingest::server::run_ingest_servers(
        store.clone(),
        grpc_addr,
        http_addr,
        PipelineConfig {
            channel_capacity: 512,
            flush_interval: std::time::Duration::from_millis(cfg.write_flush_ms),
            batch_size: cfg.write_batch_size,
        },
        cfg.forward_otlp_endpoint
            .clone()
            .map(|endpoint| ForwardConfig {
                endpoint,
                protocol: ForwardProtocol::parse(&cfg.forward_otlp_protocol),
            }),
    ));

    let query_task = tokio::spawn(query_server::run_query_server(
        store.clone(),
        cfg.uds_path.clone(),
        cfg.query_tcp_addr.parse()?,
    ));

    let query_http_task = tokio::spawn(query_server::run_query_http_server(
        store.clone(),
        cfg.query_http_addr.parse()?,
    ));

    let retention_task = tokio::spawn({
        let store = store.clone();
        let ttl = cfg.retention_ttl;
        let max = cfg.retention_max_bytes;
        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                if let Err(err) = store.run_retention(ttl, max) {
                    tracing::warn!(error = ?err, "retention task failed");
                }
            }
        }
    });

    tokio::select! {
        res = ingest_task => {
            res??;
        }
        res = query_task => {
            res??;
        }
        res = query_http_task => {
            res??;
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, shutting down");
        }
    }

    retention_task.abort();
    shutdown_tracing();
    Ok(())
}

fn parse_window(since: Option<String>, until: Option<String>) -> anyhow::Result<TimeWindow> {
    let since = since.map(|v| parse_time_or_relative(&v)).transpose()?;
    let until = until.map(|v| parse_time_or_relative(&v)).transpose()?;
    Ok(TimeWindow { since, until })
}

fn parse_sort(sort: &str) -> SortOrder {
    match sort {
        "ts_desc" => SortOrder::TsDesc,
        "duration_desc" => SortOrder::DurationDesc,
        _ => SortOrder::TsAsc,
    }
}

fn parse_logs_mode(s: &str) -> anyhow::Result<LogContextMode> {
    match s {
        "none" => Ok(LogContextMode::None),
        "all" => Ok(LogContextMode::All),
        "bounded" => Ok(LogContextMode::Bounded),
        other => anyhow::bail!("invalid logs mode: {other}"),
    }
}

fn parse_context(context: Option<String>) -> anyhow::Result<(usize, Option<i64>)> {
    let Some(c) = context else {
        return Ok((0, None));
    };

    if let Ok(lines) = c.parse::<usize>() {
        return Ok((lines, None));
    }

    let duration = parse_duration_str(&c)?;
    Ok((0, Some(duration.as_secs() as i64)))
}

fn encode_handle(req: &ApiRequest) -> anyhow::Result<String> {
    let payload = serde_json::to_vec(req)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(payload))
}

fn decode_handle(handle: &str) -> anyhow::Result<ApiRequest> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(handle)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn print_response(response: ApiResponse, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    match response {
        ApiResponse::Search(v) => print_search_human(&v),
        ApiResponse::Trace(v) => print_trace_human(&v),
        ApiResponse::Span(v) => print_span_human(&v),
        ApiResponse::Traces(v) => print_traces_human(&v),
        ApiResponse::Metrics(v) => print_metrics_human(&v),
        ApiResponse::MetricsList(v) => print_metrics_list_human(&v),
        ApiResponse::Status(v) => print_status_human(&v),
        ApiResponse::Error(e) => eprintln!("error: {e}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_logs_mode_variants() {
        assert!(matches!(
            parse_logs_mode("none").unwrap(),
            LogContextMode::None
        ));
        assert!(matches!(
            parse_logs_mode("all").unwrap(),
            LogContextMode::All
        ));
        assert!(matches!(
            parse_logs_mode("bounded").unwrap(),
            LogContextMode::Bounded
        ));
        assert!(parse_logs_mode("bad").is_err());
    }

    #[test]
    fn parse_sort_variants() {
        assert!(matches!(parse_sort("ts_desc"), SortOrder::TsDesc));
        assert!(matches!(
            parse_sort("duration_desc"),
            SortOrder::DurationDesc
        ));
        assert!(matches!(parse_sort("other"), SortOrder::TsAsc));
    }

    #[test]
    fn parse_context_lines_and_time() {
        assert_eq!(parse_context(Some("20".into())).unwrap(), (20, None));
        assert_eq!(parse_context(Some("2s".into())).unwrap(), (0, Some(2)));
        assert!(parse_context(Some("wat".into())).is_err());
    }
}
