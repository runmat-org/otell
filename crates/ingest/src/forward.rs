use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_client::MetricsServiceClient;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_client::TraceServiceClient;
use prost::Message;
use reqwest::Client;
use tokio::sync::{Mutex, mpsc};

#[derive(Debug, Clone)]
pub struct ForwardConfig {
    pub endpoint: String,
    pub protocol: ForwardProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardProtocol {
    Grpc,
    HttpProtobuf,
}

impl ForwardProtocol {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "http" | "http/protobuf" | "httpprotobuf" => Self::HttpProtobuf,
            _ => Self::Grpc,
        }
    }
}

#[derive(Clone)]
pub struct Forwarder {
    tx: mpsc::Sender<ForwardMsg>,
}

#[derive(Debug, Clone)]
enum ForwardMsg {
    Logs(ExportLogsServiceRequest),
    Traces(ExportTraceServiceRequest),
    Metrics(ExportMetricsServiceRequest),
}

pub fn build_forwarder(cfg: Option<ForwardConfig>) -> Option<Forwarder> {
    let cfg = cfg?;
    let (tx, mut rx) = mpsc::channel::<ForwardMsg>(512);

    tokio::spawn(async move {
        match cfg.protocol {
            ForwardProtocol::Grpc => {
                let endpoint = normalize_grpc_endpoint(&cfg.endpoint);
                let channel = match tonic::transport::Channel::from_shared(endpoint) {
                    Ok(c) => c.connect_lazy(),
                    Err(err) => {
                        tracing::warn!(error = ?err, "invalid gRPC forward endpoint");
                        return;
                    }
                };

                let logs_client = Mutex::new(LogsServiceClient::new(channel.clone()));
                let traces_client = Mutex::new(TraceServiceClient::new(channel.clone()));
                let metrics_client = Mutex::new(MetricsServiceClient::new(channel));

                while let Some(msg) = rx.recv().await {
                    match msg {
                        ForwardMsg::Logs(req) => {
                            forward_with_retries(|| async {
                                let mut client = logs_client.lock().await;
                                client
                                    .export(tonic::Request::new(req.clone()))
                                    .await
                                    .map(|_| ())
                            })
                            .await;
                        }
                        ForwardMsg::Traces(req) => {
                            forward_with_retries(|| async {
                                let mut client = traces_client.lock().await;
                                client
                                    .export(tonic::Request::new(req.clone()))
                                    .await
                                    .map(|_| ())
                            })
                            .await;
                        }
                        ForwardMsg::Metrics(req) => {
                            forward_with_retries(|| async {
                                let mut client = metrics_client.lock().await;
                                client
                                    .export(tonic::Request::new(req.clone()))
                                    .await
                                    .map(|_| ())
                            })
                            .await;
                        }
                    }
                }
            }
            ForwardProtocol::HttpProtobuf => {
                let endpoint = cfg.endpoint.trim_end_matches('/').to_string();
                let client = Client::new();

                while let Some(msg) = rx.recv().await {
                    match msg {
                        ForwardMsg::Logs(req) => {
                            let mut body = Vec::new();
                            if req.encode(&mut body).is_ok() {
                                let url = format!("{endpoint}/v1/logs");
                                forward_http_with_retries(&client, &url, body).await;
                            }
                        }
                        ForwardMsg::Traces(req) => {
                            let mut body = Vec::new();
                            if req.encode(&mut body).is_ok() {
                                let url = format!("{endpoint}/v1/traces");
                                forward_http_with_retries(&client, &url, body).await;
                            }
                        }
                        ForwardMsg::Metrics(req) => {
                            let mut body = Vec::new();
                            if req.encode(&mut body).is_ok() {
                                let url = format!("{endpoint}/v1/metrics");
                                forward_http_with_retries(&client, &url, body).await;
                            }
                        }
                    }
                }
            }
        }
    });

    Some(Forwarder { tx })
}

impl Forwarder {
    pub async fn submit_logs(&self, req: ExportLogsServiceRequest) {
        let _ = self.tx.send(ForwardMsg::Logs(req)).await;
    }

    pub async fn submit_traces(&self, req: ExportTraceServiceRequest) {
        let _ = self.tx.send(ForwardMsg::Traces(req)).await;
    }

    pub async fn submit_metrics(&self, req: ExportMetricsServiceRequest) {
        let _ = self.tx.send(ForwardMsg::Metrics(req)).await;
    }
}

fn normalize_grpc_endpoint(endpoint: &str) -> String {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.to_string()
    } else {
        format!("http://{endpoint}")
    }
}

async fn forward_with_retries<F, Fut, E>(mut call: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), E>>,
    E: std::fmt::Debug,
{
    for attempt in 0..3 {
        if call().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(30 * (attempt + 1) as u64)).await;
    }
    tracing::warn!("forward attempt failed after retries");
}

async fn forward_http_with_retries(client: &Client, url: &str, body: Vec<u8>) {
    for attempt in 0..3 {
        let result = client
            .post(url)
            .header("content-type", "application/x-protobuf")
            .body(body.clone())
            .send()
            .await;
        if let Ok(resp) = result
            && resp.status().is_success()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(30 * (attempt + 1) as u64)).await;
    }
    tracing::warn!(url = %url, "forward HTTP attempt failed after retries");
}
