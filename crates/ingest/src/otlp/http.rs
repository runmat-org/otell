use axum::extract::State;
use axum::http::Method;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Router, body::Bytes};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use prost::Message;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::Level;

use crate::forward::Forwarder;
use crate::otlp::decode::{decode_log, decode_metric, decode_span};
use crate::pipeline::Pipeline;

#[derive(Clone)]
pub struct HttpIngestState {
    pub pipeline: Pipeline,
    pub forwarder: Option<Forwarder>,
}

pub fn router(pipeline: Pipeline, forwarder: Option<Forwarder>) -> Router {
    let state = HttpIngestState {
        pipeline,
        forwarder,
    };
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::POST, Method::OPTIONS])
        .allow_headers(Any);
    Router::new()
        .route("/v1/logs", post(export_logs))
        .route("/v1/traces", post(export_traces))
        .route("/v1/metrics", post(export_metrics))
        .layer(cors)
        .layer(
            TraceLayer::new_for_http()
                .on_request(tower_http::trace::DefaultOnRequest::new().level(Level::INFO))
                .on_response(tower_http::trace::DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}

async fn export_logs(State(state): State<HttpIngestState>, body: Bytes) -> StatusCode {
    let Ok(req) = ExportLogsServiceRequest::decode(body) else {
        tracing::warn!("otlp http logs decode failed");
        return StatusCode::BAD_REQUEST;
    };
    if let Some(forwarder) = &state.forwarder {
        forwarder.submit_logs(req.clone()).await;
    }

    let mut logs = Vec::new();
    for rl in req.resource_logs {
        let resource = rl.resource.as_ref();
        for sl in rl.scope_logs {
            let scope = sl.scope.as_ref();
            for log in sl.log_records {
                logs.push(decode_log(resource, scope, &log));
            }
        }
    }
    tracing::debug!(count = logs.len(), "otlp http logs accepted");
    state.pipeline.submit_logs(logs).await;
    StatusCode::OK
}

async fn export_traces(State(state): State<HttpIngestState>, body: Bytes) -> StatusCode {
    let Ok(req) = ExportTraceServiceRequest::decode(body) else {
        tracing::warn!("otlp http traces decode failed");
        return StatusCode::BAD_REQUEST;
    };
    if let Some(forwarder) = &state.forwarder {
        forwarder.submit_traces(req.clone()).await;
    }

    let mut spans = Vec::new();
    for rs in req.resource_spans {
        let resource = rs.resource.as_ref();
        for ss in rs.scope_spans {
            for span in ss.spans {
                spans.push(decode_span(resource, &span));
            }
        }
    }
    tracing::debug!(count = spans.len(), "otlp http traces accepted");
    state.pipeline.submit_spans(spans).await;
    StatusCode::OK
}

async fn export_metrics(State(state): State<HttpIngestState>, body: Bytes) -> StatusCode {
    let Ok(req) = ExportMetricsServiceRequest::decode(body) else {
        tracing::warn!("otlp http metrics decode failed");
        return StatusCode::BAD_REQUEST;
    };
    if let Some(forwarder) = &state.forwarder {
        forwarder.submit_metrics(req.clone()).await;
    }

    let mut points = Vec::new();
    for rm in req.resource_metrics {
        let resource = rm.resource.as_ref();
        for sm in rm.scope_metrics {
            for metric in sm.metrics {
                if let Some(data) = &metric.data {
                    if let opentelemetry_proto::tonic::metrics::v1::metric::Data::Gauge(g) = data {
                        for point in &g.data_points {
                            points.push(decode_metric(resource, &metric, point));
                        }
                    }
                    if let opentelemetry_proto::tonic::metrics::v1::metric::Data::Sum(s) = data {
                        for point in &s.data_points {
                            points.push(decode_metric(resource, &metric, point));
                        }
                    }
                }
            }
        }
    }
    tracing::debug!(count = points.len(), "otlp http metrics accepted");
    state.pipeline.submit_metrics(points).await;
    StatusCode::OK
}
