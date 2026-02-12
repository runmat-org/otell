use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
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

fn handle_request(req: ApiRequest, store: &otell_store::Store) -> ApiResponse {
    let resp = match req {
        ApiRequest::Search(r) => store.search_logs(&r).map(ApiResponse::Search),
        ApiRequest::Trace(r) => store.get_trace(&r).map(ApiResponse::Trace),
        ApiRequest::Span(r) => store.get_span(&r).map(ApiResponse::Span),
        ApiRequest::Traces(r) => store.list_traces(&r).map(ApiResponse::Traces),
        ApiRequest::Metrics(r) => store.query_metrics(&r).map(ApiResponse::Metrics),
        ApiRequest::Status => store.status().map(ApiResponse::Status),
    };

    resp.unwrap_or_else(|e| ApiResponse::Error(e.to_string()))
}
