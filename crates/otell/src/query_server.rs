use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use otell_core::filter::Severity;
use otell_core::model::log::LogRecord;
use otell_core::query::{
    MetricsListRequest, MetricsRequest, QueryHandle, SearchRequest, SpanRequest, TraceRequest,
    TracesRequest,
};
use regex::RegexBuilder;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener};
use tower_http::trace::TraceLayer;
use tracing::Level;

use crate::protocol::{ApiRequest, ApiResponse};

pub async fn run_query_server(
    store: otell_store::Store,
    uds_path: PathBuf,
    tcp_addr: SocketAddr,
) -> anyhow::Result<()> {
    if let Some(parent) = uds_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("create uds parent dir")?;
    }

    if tokio::fs::metadata(&uds_path).await.is_ok() {
        let _ = tokio::fs::remove_file(&uds_path).await;
    }

    let uds_listener = UnixListener::bind(&uds_path).context("bind UDS query listener")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&uds_path).await?.permissions();
        perms.set_mode(0o600);
        tokio::fs::set_permissions(&uds_path, perms).await?;
    }
    let tcp_listener = TcpListener::bind(tcp_addr)
        .await
        .context("bind TCP query listener")?;

    tracing::info!(path = %uds_path.display(), "query UDS server listening");
    tracing::info!(addr = %tcp_addr, "query TCP server listening");

    let uds_task = tokio::spawn(run_uds_loop(uds_listener, store.clone()));
    let tcp_task = tokio::spawn(run_tcp_loop(tcp_listener, store));

    tokio::select! {
        res = uds_task => {
            res??;
        }
        res = tcp_task => {
            res??;
        }
    }

    Ok(())
}

pub async fn run_query_http_server(
    store: otell_store::Store,
    http_addr: SocketAddr,
) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/v1/search", post(http_search))
        .route("/v1/trace", post(http_trace))
        .route("/v1/trace/{trace_id}", get(http_trace_get))
        .route("/v1/span", post(http_span))
        .route("/v1/traces", post(http_traces))
        .route("/v1/metrics", post(http_metrics))
        .route("/v1/metrics/list", post(http_metrics_list))
        .route("/v1/status", get(http_status))
        .route("/v1/tail", get(http_tail))
        .layer(
            TraceLayer::new_for_http()
                .on_request(tower_http::trace::DefaultOnRequest::new().level(Level::INFO))
                .on_response(tower_http::trace::DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(store);

    let listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .context("bind HTTP query listener")?;
    tracing::info!(addr = %http_addr, "query HTTP server listening");
    axum::serve(listener, app)
        .await
        .context("run HTTP query server")
}

async fn run_uds_loop(listener: UnixListener, store: otell_store::Store) -> anyhow::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let store = store.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_stream(BufReader::new(stream), store).await {
                tracing::warn!(error = ?err, "uds client request failed");
            }
        });
    }
}

async fn run_tcp_loop(listener: TcpListener, store: otell_store::Store) -> anyhow::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let store = store.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_stream(BufReader::new(stream), store).await {
                tracing::warn!(error = ?err, "tcp client request failed");
            }
        });
    }
}

async fn handle_stream<T>(mut stream: BufReader<T>, store: otell_store::Store) -> anyhow::Result<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut line = String::new();
    let n = stream.read_line(&mut line).await?;
    if n == 0 {
        return Ok(());
    }

    let req: ApiRequest = serde_json::from_str(&line)?;
    let response = handle_request(req, &store);
    let payload = serde_json::to_vec(&response)?;
    stream.get_mut().write_all(&payload).await?;
    stream.get_mut().write_all(b"\n").await?;
    stream.get_mut().flush().await?;
    Ok(())
}

pub fn handle_request(req: ApiRequest, store: &otell_store::Store) -> ApiResponse {
    let resp = match req {
        ApiRequest::Search(r) => store.search_logs(&r).map(ApiResponse::Search),
        ApiRequest::Trace(r) => store.get_trace(&r).map(ApiResponse::Trace),
        ApiRequest::Span(r) => store.get_span(&r).map(ApiResponse::Span),
        ApiRequest::Traces(r) => store.list_traces(&r).map(ApiResponse::Traces),
        ApiRequest::Metrics(r) => store.query_metrics(&r).map(ApiResponse::Metrics),
        ApiRequest::MetricsList(r) => store.list_metric_names(&r).map(ApiResponse::MetricsList),
        ApiRequest::ResolveHandle(handle) => resolve_handle(handle, store),
        ApiRequest::Status => store.status().map(ApiResponse::Status),
    };
    match resp {
        Ok(value) => value,
        Err(e) => ApiResponse::Error(e.to_string()),
    }
}

fn resolve_handle(
    handle: QueryHandle,
    store: &otell_store::Store,
) -> otell_core::Result<ApiResponse> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(handle.handle)
        .map_err(|e| otell_core::OtellError::Parse(format!("invalid handle: {e}")))?;
    let req: ApiRequest = serde_json::from_slice(&bytes)
        .map_err(|e| otell_core::OtellError::Parse(format!("invalid handle payload: {e}")))?;
    Ok(handle_request(req, store))
}

async fn http_search(
    State(store): State<otell_store::Store>,
    Json(req): Json<SearchRequest>,
) -> Json<ApiResponse> {
    tracing::debug!(limit = req.limit, "http query search request");
    Json(handle_request(ApiRequest::Search(req), &store))
}

async fn http_trace(
    State(store): State<otell_store::Store>,
    Json(req): Json<TraceRequest>,
) -> Json<ApiResponse> {
    tracing::debug!(trace_id = %req.trace_id, "http query trace request");
    Json(handle_request(ApiRequest::Trace(req), &store))
}

async fn http_trace_get(
    State(store): State<otell_store::Store>,
    Path(trace_id): Path<String>,
) -> Json<ApiResponse> {
    tracing::debug!(trace_id = %trace_id, "http query trace get request");
    Json(handle_request(
        ApiRequest::Trace(TraceRequest {
            trace_id,
            root_span_id: None,
            logs: otell_core::query::LogContextMode::Bounded,
        }),
        &store,
    ))
}

async fn http_span(
    State(store): State<otell_store::Store>,
    Json(req): Json<SpanRequest>,
) -> Json<ApiResponse> {
    tracing::debug!(trace_id = %req.trace_id, span_id = %req.span_id, "http query span request");
    Json(handle_request(ApiRequest::Span(req), &store))
}

async fn http_traces(
    State(store): State<otell_store::Store>,
    Json(req): Json<TracesRequest>,
) -> Json<ApiResponse> {
    tracing::debug!(limit = req.limit, "http query traces request");
    Json(handle_request(ApiRequest::Traces(req), &store))
}

async fn http_metrics(
    State(store): State<otell_store::Store>,
    Json(req): Json<MetricsRequest>,
) -> Json<ApiResponse> {
    tracing::debug!(name = %req.name, limit = req.limit, "http query metrics request");
    Json(handle_request(ApiRequest::Metrics(req), &store))
}

async fn http_metrics_list(
    State(store): State<otell_store::Store>,
    Json(req): Json<MetricsListRequest>,
) -> Json<ApiResponse> {
    tracing::debug!(limit = req.limit, "http query metrics list request");
    Json(handle_request(ApiRequest::MetricsList(req), &store))
}

async fn http_status(State(store): State<otell_store::Store>) -> Json<ApiResponse> {
    tracing::debug!("http query status request");
    Json(handle_request(ApiRequest::Status, &store))
}

#[derive(Debug, Clone, serde::Deserialize)]
struct TailQuery {
    pattern: Option<String>,
    fixed: Option<bool>,
    ignore_case: Option<bool>,
    service: Option<String>,
    trace_id: Option<String>,
    span_id: Option<String>,
    severity: Option<String>,
}

async fn http_tail(
    State(store): State<otell_store::Store>,
    Query(query): Query<TailQuery>,
) -> Sse<impl futures::Stream<Item = std::result::Result<Event, std::convert::Infallible>>> {
    tracing::info!(?query, "http query tail stream opened");
    let mut rx = store.subscribe_logs();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(record) => {
                    if !matches_tail_query(&record, &query) {
                        continue;
                    }
                    if let Ok(data) = serde_json::to_string(&record) {
                        yield Ok(Event::default().data(data));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn matches_tail_query(record: &LogRecord, query: &TailQuery) -> bool {
    if let Some(service) = &query.service
        && &record.service != service
    {
        return false;
    }
    if let Some(trace_id) = &query.trace_id
        && record.trace_id.as_deref() != Some(trace_id.as_str())
    {
        return false;
    }
    if let Some(span_id) = &query.span_id
        && record.span_id.as_deref() != Some(span_id.as_str())
    {
        return false;
    }
    if let Some(severity) = &query.severity {
        let parsed = severity
            .parse::<Severity>()
            .ok()
            .map(|s| s as i32)
            .unwrap_or(0);
        if record.severity < parsed {
            return false;
        }
    }
    if let Some(pattern) = &query.pattern {
        if query.fixed.unwrap_or(false) {
            let needle = if query.ignore_case.unwrap_or(false) {
                pattern.to_ascii_lowercase()
            } else {
                pattern.clone()
            };
            let haystack = if query.ignore_case.unwrap_or(false) {
                record.body.to_ascii_lowercase()
            } else {
                record.body.clone()
            };
            return haystack.contains(&needle);
        }
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(query.ignore_case.unwrap_or(false))
            .build();
        if let Ok(regex) = regex {
            return regex.is_match(&record.body);
        }
        return false;
    }

    true
}
