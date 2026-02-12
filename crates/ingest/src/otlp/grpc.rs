use std::sync::Arc;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_server::{
    LogsService, LogsServiceServer,
};
use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_server::{
    MetricsService, MetricsServiceServer,
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::{
    TraceService, TraceServiceServer,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use tonic::{Request, Response, Status};

use crate::otlp::decode::{decode_log, decode_metric, decode_span};
use crate::pipeline::Pipeline;

#[derive(Clone)]
pub struct GrpcIngest {
    pipeline: Arc<Pipeline>,
}

impl GrpcIngest {
    pub fn new(pipeline: Pipeline) -> Self {
        Self {
            pipeline: Arc::new(pipeline),
        }
    }

    pub fn logs_service(&self) -> LogsServiceServer<Self> {
        LogsServiceServer::new(self.clone())
    }

    pub fn traces_service(&self) -> TraceServiceServer<Self> {
        TraceServiceServer::new(self.clone())
    }

    pub fn metrics_service(&self) -> MetricsServiceServer<Self> {
        MetricsServiceServer::new(self.clone())
    }
}

#[tonic::async_trait]
impl LogsService for GrpcIngest {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> std::result::Result<Response<ExportLogsServiceResponse>, Status> {
        let req = request.into_inner();
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
        self.pipeline.submit_logs(logs).await;
        Ok(Response::new(ExportLogsServiceResponse::default()))
    }
}

#[tonic::async_trait]
impl TraceService for GrpcIngest {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> std::result::Result<Response<ExportTraceServiceResponse>, Status> {
        let req = request.into_inner();
        let mut spans = Vec::new();
        for rs in req.resource_spans {
            let resource = rs.resource.as_ref();
            for ss in rs.scope_spans {
                for span in ss.spans {
                    spans.push(decode_span(resource, &span));
                }
            }
        }
        self.pipeline.submit_spans(spans).await;
        Ok(Response::new(ExportTraceServiceResponse::default()))
    }
}

#[tonic::async_trait]
impl MetricsService for GrpcIngest {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> std::result::Result<Response<ExportMetricsServiceResponse>, Status> {
        let req = request.into_inner();
        let mut points = Vec::new();
        for rm in req.resource_metrics {
            let resource = rm.resource.as_ref();
            for sm in rm.scope_metrics {
                for metric in sm.metrics {
                    if let Some(data) = &metric.data {
                        if let opentelemetry_proto::tonic::metrics::v1::metric::Data::Gauge(g) =
                            data
                        {
                            for point in &g.data_points {
                                points.push(decode_metric(resource, &metric, point));
                            }
                        }
                        if let opentelemetry_proto::tonic::metrics::v1::metric::Data::Sum(s) = data
                        {
                            for point in &s.data_points {
                                points.push(decode_metric(resource, &metric, point));
                            }
                        }
                    }
                }
            }
        }
        self.pipeline.submit_metrics(points).await;
        Ok(Response::new(ExportMetricsServiceResponse::default()))
    }
}
