use otell_core::query::{
    MetricsRequest, MetricsResponse, SearchRequest, SearchResponse, SpanRequest, SpanResponse,
    StatusResponse, TraceListItem, TraceRequest, TraceResponse, TracesRequest,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiRequest {
    Search(SearchRequest),
    Trace(TraceRequest),
    Span(SpanRequest),
    Traces(TracesRequest),
    Metrics(MetricsRequest),
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiResponse {
    Search(SearchResponse),
    Trace(TraceResponse),
    Span(SpanResponse),
    Traces(Vec<TraceListItem>),
    Metrics(MetricsResponse),
    Status(StatusResponse),
    Error(String),
}
