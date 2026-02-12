use otell_core::query::{
    MetricsListRequest, MetricsListResponse, MetricsRequest, MetricsResponse, QueryHandle,
    SearchRequest, SearchResponse, SpanRequest, SpanResponse, StatusResponse, TraceListItem,
    TraceRequest, TraceResponse, TracesRequest,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiRequest {
    Search(SearchRequest),
    Trace(TraceRequest),
    Span(SpanRequest),
    Traces(TracesRequest),
    Metrics(MetricsRequest),
    MetricsList(MetricsListRequest),
    ResolveHandle(QueryHandle),
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiResponse {
    Search(SearchResponse),
    Trace(TraceResponse),
    Span(SpanResponse),
    Traces(Vec<TraceListItem>),
    Metrics(MetricsResponse),
    MetricsList(MetricsListResponse),
    Status(StatusResponse),
    Error(String),
}
