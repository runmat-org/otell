use std::net::SocketAddr;

use otell_core::error::{OtellError, Result};
use tonic::transport::Server;

use crate::otlp::grpc::GrpcIngest;
use crate::otlp::http;
use crate::pipeline::{Pipeline, PipelineConfig};

pub async fn run_ingest_servers(
    store: otell_store::Store,
    grpc_addr: SocketAddr,
    http_addr: SocketAddr,
    cfg: PipelineConfig,
) -> Result<()> {
    let pipeline = Pipeline::new(store, cfg);
    let grpc = GrpcIngest::new(pipeline.clone());
    let http_router = http::router(pipeline);

    let grpc_task = tokio::spawn(async move {
        Server::builder()
            .add_service(grpc.logs_service())
            .add_service(grpc.traces_service())
            .add_service(grpc.metrics_service())
            .serve(grpc_addr)
            .await
    });

    let http_task = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr).await?;
        axum::serve(listener, http_router).await
    });

    tokio::select! {
        res = grpc_task => {
            let inner = res.map_err(|e| OtellError::Ingest(format!("gRPC task join failed: {e}")))?;
            inner.map_err(|e| OtellError::Ingest(format!("gRPC server failed: {e}")))
        }
        res = http_task => {
            let inner = res.map_err(|e| OtellError::Ingest(format!("HTTP task join failed: {e}")))?;
            inner.map_err(|e| OtellError::Ingest(format!("HTTP server failed: {e}")))
        }
    }
}
