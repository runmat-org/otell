use std::time::Duration;
use std::{io::Write, sync::Arc};

use flate2::Compression;
use flate2::write::GzEncoder;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_client::MetricsServiceClient;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_client::TraceServiceClient;
use prost::Message;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::sync::{Mutex, mpsc};
use tonic::codec::CompressionEncoding;
use tonic::metadata::{Ascii, MetadataKey, MetadataMap, MetadataValue};

#[derive(Debug, Clone)]
pub struct ForwardConfig {
    pub endpoint: String,
    pub protocol: ForwardProtocol,
    pub compression: ForwardCompression,
    pub headers: Vec<(String, String)>,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardProtocol {
    Grpc,
    HttpProtobuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardCompression {
    None,
    Gzip,
}

impl ForwardProtocol {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "http" | "http/protobuf" | "httpprotobuf" => Self::HttpProtobuf,
            _ => Self::Grpc,
        }
    }
}

impl ForwardCompression {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "gzip" => Self::Gzip,
            _ => Self::None,
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

                let logs_client = Mutex::new(configure_logs_client(
                    LogsServiceClient::new(channel.clone()),
                    cfg.compression,
                ));
                let traces_client = Mutex::new(configure_traces_client(
                    TraceServiceClient::new(channel.clone()),
                    cfg.compression,
                ));
                let metrics_client = Mutex::new(configure_metrics_client(
                    MetricsServiceClient::new(channel),
                    cfg.compression,
                ));
                let grpc_metadata = Arc::new(build_grpc_metadata(&cfg.headers));
                let timeout = cfg.timeout;

                while let Some(msg) = rx.recv().await {
                    match msg {
                        ForwardMsg::Logs(req) => {
                            forward_with_retries(|| async {
                                let mut client = logs_client.lock().await;
                                let mut request = tonic::Request::new(req.clone());
                                request.set_timeout(timeout);
                                *request.metadata_mut() = (*grpc_metadata).clone();
                                client.export(request).await.map(|_| ())
                            })
                            .await;
                        }
                        ForwardMsg::Traces(req) => {
                            forward_with_retries(|| async {
                                let mut client = traces_client.lock().await;
                                let mut request = tonic::Request::new(req.clone());
                                request.set_timeout(timeout);
                                *request.metadata_mut() = (*grpc_metadata).clone();
                                client.export(request).await.map(|_| ())
                            })
                            .await;
                        }
                        ForwardMsg::Metrics(req) => {
                            forward_with_retries(|| async {
                                let mut client = metrics_client.lock().await;
                                let mut request = tonic::Request::new(req.clone());
                                request.set_timeout(timeout);
                                *request.metadata_mut() = (*grpc_metadata).clone();
                                client.export(request).await.map(|_| ())
                            })
                            .await;
                        }
                    }
                }
            }
            ForwardProtocol::HttpProtobuf => {
                let endpoint = cfg.endpoint.trim_end_matches('/').to_string();
                let client = Client::builder()
                    .timeout(cfg.timeout)
                    .build()
                    .unwrap_or_else(|e| {
                        tracing::warn!(error = ?e, "failed to build forward http client; using defaults");
                        Client::new()
                    });
                let headers = build_http_headers(&cfg.headers);
                let compression = cfg.compression;

                while let Some(msg) = rx.recv().await {
                    match msg {
                        ForwardMsg::Logs(req) => {
                            let mut body = Vec::new();
                            if req.encode(&mut body).is_ok() {
                                let url = format!("{endpoint}/v1/logs");
                                forward_http_with_retries(
                                    &client,
                                    &url,
                                    &headers,
                                    body,
                                    compression,
                                )
                                .await;
                            }
                        }
                        ForwardMsg::Traces(req) => {
                            let mut body = Vec::new();
                            if req.encode(&mut body).is_ok() {
                                let url = format!("{endpoint}/v1/traces");
                                forward_http_with_retries(
                                    &client,
                                    &url,
                                    &headers,
                                    body,
                                    compression,
                                )
                                .await;
                            }
                        }
                        ForwardMsg::Metrics(req) => {
                            let mut body = Vec::new();
                            if req.encode(&mut body).is_ok() {
                                let url = format!("{endpoint}/v1/metrics");
                                forward_http_with_retries(
                                    &client,
                                    &url,
                                    &headers,
                                    body,
                                    compression,
                                )
                                .await;
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

async fn forward_http_with_retries(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    body: Vec<u8>,
    compression: ForwardCompression,
) {
    let Ok((body, content_encoding)) = maybe_compress_http_body(body, compression) else {
        tracing::warn!(url = %url, "failed to compress forward HTTP payload");
        return;
    };

    for attempt in 0..3 {
        let mut req = client
            .post(url)
            .header("content-type", "application/x-protobuf")
            .headers(headers.clone());
        if let Some(encoding) = content_encoding {
            req = req.header("content-encoding", encoding);
        }
        let result = req.body(body.clone()).send().await;
        if let Ok(resp) = result
            && resp.status().is_success()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(30 * (attempt + 1) as u64)).await;
    }
    tracing::warn!(url = %url, "forward HTTP attempt failed after retries");
}

fn configure_logs_client(
    client: LogsServiceClient<tonic::transport::Channel>,
    compression: ForwardCompression,
) -> LogsServiceClient<tonic::transport::Channel> {
    match compression {
        ForwardCompression::Gzip => client
            .send_compressed(CompressionEncoding::Gzip)
            .accept_compressed(CompressionEncoding::Gzip),
        ForwardCompression::None => client,
    }
}

fn configure_traces_client(
    client: TraceServiceClient<tonic::transport::Channel>,
    compression: ForwardCompression,
) -> TraceServiceClient<tonic::transport::Channel> {
    match compression {
        ForwardCompression::Gzip => client
            .send_compressed(CompressionEncoding::Gzip)
            .accept_compressed(CompressionEncoding::Gzip),
        ForwardCompression::None => client,
    }
}

fn configure_metrics_client(
    client: MetricsServiceClient<tonic::transport::Channel>,
    compression: ForwardCompression,
) -> MetricsServiceClient<tonic::transport::Channel> {
    match compression {
        ForwardCompression::Gzip => client
            .send_compressed(CompressionEncoding::Gzip)
            .accept_compressed(CompressionEncoding::Gzip),
        ForwardCompression::None => client,
    }
}

fn build_grpc_metadata(headers: &[(String, String)]) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    for (k, v) in headers {
        let key = MetadataKey::<Ascii>::from_bytes(k.as_bytes());
        let value = MetadataValue::try_from(v.as_str());
        match (key, value) {
            (Ok(key), Ok(value)) => {
                metadata.insert(key, value);
            }
            _ => {
                tracing::warn!(header = %k, "ignored invalid forward gRPC header");
            }
        }
    }
    metadata
}

fn build_http_headers(headers: &[(String, String)]) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (k, v) in headers {
        let name = HeaderName::try_from(k.as_str());
        let value = HeaderValue::try_from(v.as_str());
        match (name, value) {
            (Ok(name), Ok(value)) => {
                out.insert(name, value);
            }
            _ => {
                tracing::warn!(header = %k, "ignored invalid forward HTTP header");
            }
        }
    }
    out
}

fn maybe_compress_http_body(
    body: Vec<u8>,
    compression: ForwardCompression,
) -> std::io::Result<(Vec<u8>, Option<&'static str>)> {
    match compression {
        ForwardCompression::None => Ok((body, None)),
        ForwardCompression::Gzip => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&body)?;
            let compressed = encoder.finish()?;
            Ok((compressed, Some("gzip")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_compression_parse_variants() {
        assert_eq!(ForwardCompression::parse("gzip"), ForwardCompression::Gzip);
        assert_eq!(ForwardCompression::parse("GZIP"), ForwardCompression::Gzip);
        assert_eq!(ForwardCompression::parse("none"), ForwardCompression::None);
        assert_eq!(
            ForwardCompression::parse("unexpected"),
            ForwardCompression::None
        );
    }
}
