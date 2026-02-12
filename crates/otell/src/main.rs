mod client;
mod output;
mod protocol;
mod query_server;

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use clap::{Parser, Subcommand};
use otell_core::config::Config;
use otell_core::filter::{AttrFilter, Severity, SortOrder, TimeWindow};
use otell_core::query::{
    LogContextMode, MetricsRequest, SearchRequest, SpanRequest, TraceRequest, TracesRequest,
};
use otell_core::time::parse_time_or_relative;
use otell_ingest::pipeline::PipelineConfig;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing_subscriber::EnvFilter;

use crate::client::QueryClient;
use crate::output::{
    print_metrics_human, print_search_human, print_span_human, print_status_human,
    print_trace_human, print_traces_human,
};
use crate::protocol::{ApiRequest, ApiResponse};

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
        query_uds_path: Option<PathBuf>,
    },
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
        #[arg(short = 'C', default_value_t = 0)]
        context: usize,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value = "ts_asc")]
        sort: String,
    },
    Trace {
        trace_id: String,
        #[arg(long)]
        root: Option<String>,
        #[arg(long, default_value = "bounded")]
        logs: String,
    },
    Span {
        trace_id: String,
        span_id: String,
        #[arg(long, default_value = "bounded")]
        logs: String,
    },
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
    Metrics {
        name: String,
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
    Status,
    Doctor,
    Mcp,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            db_path,
            otlp_grpc_addr,
            otlp_http_addr,
            query_tcp_addr,
            query_uds_path,
        } => {
            run_server(
                db_path,
                otlp_grpc_addr,
                otlp_http_addr,
                query_tcp_addr,
                query_uds_path,
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
            limit,
            sort,
        } => {
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
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
                context_lines: context,
            };
            let response = client.request(ApiRequest::Search(req)).await?;
            print_response(response, cli.json)?;
            Ok(())
        }
        Commands::Trace {
            trace_id,
            root,
            logs,
        } => {
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let req = TraceRequest {
                trace_id,
                root_span_id: root,
                logs: parse_logs_mode(&logs)?,
            };
            let response = client.request(ApiRequest::Trace(req)).await?;
            print_response(response, cli.json)?;
            Ok(())
        }
        Commands::Span {
            trace_id,
            span_id,
            logs,
        } => {
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let req = SpanRequest {
                trace_id,
                span_id,
                logs: parse_logs_mode(&logs)?,
            };
            let response = client.request(ApiRequest::Span(req)).await?;
            print_response(response, cli.json)?;
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
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let req = TracesRequest {
                service,
                status,
                window: parse_window(since, until)?,
                sort: parse_sort(&sort),
                limit,
            };
            let response = client.request(ApiRequest::Traces(req)).await?;
            print_response(response, cli.json)?;
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
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let req = MetricsRequest {
                name,
                service,
                window: parse_window(since, until)?,
                group_by,
                agg,
                limit,
            };
            let response = client.request(ApiRequest::Metrics(req)).await?;
            print_response(response, cli.json)?;
            Ok(())
        }
        Commands::Status => {
            let mut client = QueryClient::connect(cli.uds, cli.addr).await?;
            let response = client.request(ApiRequest::Status).await?;
            print_response(response, cli.json)?;
            Ok(())
        }
        Commands::Doctor => {
            print_doctor();
            Ok(())
        }
        Commands::Mcp => run_mcp(cli.uds, cli.addr).await,
    }
}

async fn run_mcp(uds: Option<PathBuf>, addr: Option<String>) -> anyhow::Result<()> {
    #[derive(serde::Deserialize)]
    struct McpIn {
        tool: String,
        args: serde_json::Value,
    }

    let mut client = QueryClient::connect(uds, addr).await?;
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        let input: Result<McpIn, _> = serde_json::from_str(&line);
        let input = match input {
            Ok(v) => v,
            Err(e) => {
                println!(
                    "{}",
                    serde_json::to_string(&ApiResponse::Error(e.to_string()))?
                );
                continue;
            }
        };

        let request = match input.tool.as_str() {
            "search" => serde_json::from_value::<SearchRequest>(input.args).map(ApiRequest::Search),
            "trace" => serde_json::from_value::<TraceRequest>(input.args).map(ApiRequest::Trace),
            "span" => serde_json::from_value::<SpanRequest>(input.args).map(ApiRequest::Span),
            "traces" => serde_json::from_value::<TracesRequest>(input.args).map(ApiRequest::Traces),
            "metrics" => {
                serde_json::from_value::<MetricsRequest>(input.args).map(ApiRequest::Metrics)
            }
            "status" => Ok(ApiRequest::Status),
            _ => {
                println!(
                    "{}",
                    serde_json::to_string(&ApiResponse::Error("unknown mcp tool".to_string()))?
                );
                continue;
            }
        };

        let response = match request {
            Ok(req) => client
                .request(req)
                .await
                .unwrap_or_else(|e| ApiResponse::Error(e.to_string())),
            Err(e) => ApiResponse::Error(format!("invalid tool arguments: {e}")),
        };

        println!("{}", serde_json::to_string(&response)?);
    }

    Ok(())
}

async fn run_server(
    db_path: Option<PathBuf>,
    otlp_grpc_addr: Option<String>,
    otlp_http_addr: Option<String>,
    query_tcp_addr: Option<String>,
    query_uds_path: Option<PathBuf>,
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
    if let Some(v) = query_uds_path {
        cfg.uds_path = v;
    }

    let store = otell_store::Store::open(&cfg.db_path)?;

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
    ));

    let query_task = tokio::spawn(query_server::run_query_server(
        store.clone(),
        cfg.uds_path.clone(),
        cfg.query_tcp_addr.parse()?,
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
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, shutting down");
        }
    }

    retention_task.abort();
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
        ApiResponse::Status(v) => print_status_human(&v),
        ApiResponse::Error(e) => eprintln!("error: {e}"),
    }
    Ok(())
}

fn print_doctor() {
    println!("otell doctor");
    println!("OTLP gRPC endpoint: 127.0.0.1:4317");
    println!("OTLP HTTP endpoint: http://127.0.0.1:4318/v1/*");
    println!("set OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318");
    println!("set OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf");
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
}
