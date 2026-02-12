use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use otell_core::query::{
    MetricsListRequest, MetricsRequest, QueryHandle, SearchRequest, SpanRequest, TraceRequest,
    TracesRequest,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener};

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
        .with_state(store);

    let listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .context("bind HTTP query listener")?;
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

    resp.unwrap_or_else(|e| ApiResponse::Error(e.to_string()))
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
    Json(handle_request(ApiRequest::Search(req), &store))
}

async fn http_trace(
    State(store): State<otell_store::Store>,
    Json(req): Json<TraceRequest>,
) -> Json<ApiResponse> {
    Json(handle_request(ApiRequest::Trace(req), &store))
}

async fn http_trace_get(
    State(store): State<otell_store::Store>,
    Path(trace_id): Path<String>,
) -> Json<ApiResponse> {
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
    Json(handle_request(ApiRequest::Span(req), &store))
}

async fn http_traces(
    State(store): State<otell_store::Store>,
    Json(req): Json<TracesRequest>,
) -> Json<ApiResponse> {
    Json(handle_request(ApiRequest::Traces(req), &store))
}

async fn http_metrics(
    State(store): State<otell_store::Store>,
    Json(req): Json<MetricsRequest>,
) -> Json<ApiResponse> {
    Json(handle_request(ApiRequest::Metrics(req), &store))
}

async fn http_metrics_list(
    State(store): State<otell_store::Store>,
    Json(req): Json<MetricsListRequest>,
) -> Json<ApiResponse> {
    Json(handle_request(ApiRequest::MetricsList(req), &store))
}

async fn http_status(State(store): State<otell_store::Store>) -> Json<ApiResponse> {
    Json(handle_request(ApiRequest::Status, &store))
}
