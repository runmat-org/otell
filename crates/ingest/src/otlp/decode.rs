use chrono::{TimeZone, Utc};
use opentelemetry_proto::tonic::common::v1::{AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::LogRecord as OtlpLogRecord;
use opentelemetry_proto::tonic::metrics::v1::{Metric, NumberDataPoint};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::Span as OtlpSpan;
use otell_core::model::log::LogRecord;
use otell_core::model::metric::MetricPoint;
use otell_core::model::span::SpanRecord;

pub fn decode_log(
    resource: Option<&Resource>,
    _scope: Option<&InstrumentationScope>,
    record: &OtlpLogRecord,
) -> LogRecord {
    let attrs = kv_to_json(&record.attributes);
    let attrs_text = json_to_attr_text(&attrs);
    let service = service_name(resource);
    let ts_nanos = if record.time_unix_nano == 0 {
        record.observed_time_unix_nano
    } else {
        record.time_unix_nano
    };

    LogRecord {
        ts: nanos_to_dt(ts_nanos),
        service,
        severity: record.severity_number,
        trace_id: bytes_to_hex(&record.trace_id),
        span_id: bytes_to_hex(&record.span_id),
        body: any_value_to_string(record.body.as_ref()),
        attrs_json: attrs.to_string(),
        attrs_text,
    }
}

pub fn decode_span(resource: Option<&Resource>, span: &OtlpSpan) -> SpanRecord {
    let attrs = kv_to_json(&span.attributes);
    let events = serde_json::Value::Array(
        span.events
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "time_unix_nano": e.time_unix_nano,
                    "attributes": kv_to_json(&e.attributes),
                })
            })
            .collect(),
    );

    let status = span
        .status
        .as_ref()
        .map(|s| s.message.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if span.status.as_ref().map(|s| s.code).unwrap_or_default() == 2 {
                "ERROR".to_string()
            } else {
                "OK".to_string()
            }
        });

    SpanRecord {
        trace_id: bytes_to_hex(&span.trace_id).unwrap_or_default(),
        span_id: bytes_to_hex(&span.span_id).unwrap_or_default(),
        parent_span_id: bytes_to_hex(&span.parent_span_id),
        service: service_name(resource),
        name: span.name.clone(),
        start_ts: nanos_to_dt(span.start_time_unix_nano),
        end_ts: nanos_to_dt(span.end_time_unix_nano),
        status,
        attrs_json: attrs.to_string(),
        events_json: events.to_string(),
    }
}

pub fn decode_metric(
    resource: Option<&Resource>,
    metric: &Metric,
    point: &NumberDataPoint,
) -> MetricPoint {
    let value = point
        .value
        .as_ref()
        .map(|v| match v {
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsDouble(d) => *d,
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsInt(i) => {
                *i as f64
            }
        })
        .unwrap_or(0.0);

    MetricPoint {
        ts: nanos_to_dt(point.time_unix_nano),
        name: metric.name.clone(),
        service: service_name(resource),
        value,
        attrs_json: kv_to_json(&point.attributes).to_string(),
    }
}

fn service_name(resource: Option<&Resource>) -> String {
    if let Some(resource) = resource {
        for kv in &resource.attributes {
            if kv.key == "service.name" {
                return any_value_to_string(kv.value.as_ref());
            }
        }
    }
    "unknown".to_string()
}

fn kv_to_json(attrs: &[KeyValue]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for kv in attrs {
        map.insert(
            kv.key.clone(),
            serde_json::Value::String(any_value_to_string(kv.value.as_ref())),
        );
    }
    serde_json::Value::Object(map)
}

fn any_value_to_string(value: Option<&AnyValue>) -> String {
    value
        .and_then(|v| v.value.as_ref())
        .map(|v| match v {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(s) => s.clone(),
            opentelemetry_proto::tonic::common::v1::any_value::Value::BoolValue(b) => b.to_string(),
            opentelemetry_proto::tonic::common::v1::any_value::Value::IntValue(i) => i.to_string(),
            opentelemetry_proto::tonic::common::v1::any_value::Value::DoubleValue(d) => {
                d.to_string()
            }
            opentelemetry_proto::tonic::common::v1::any_value::Value::BytesValue(b) => {
                String::from_utf8_lossy(b).to_string()
            }
            _ => "<complex>".to_string(),
        })
        .unwrap_or_default()
}

fn json_to_attr_text(value: &serde_json::Value) -> String {
    value
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn bytes_to_hex(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

fn nanos_to_dt(nanos: u64) -> chrono::DateTime<Utc> {
    let secs = (nanos / 1_000_000_000) as i64;
    let subnanos = (nanos % 1_000_000_000) as u32;
    Utc.timestamp_opt(secs, subnanos)
        .single()
        .unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use opentelemetry_proto::tonic::common::v1::any_value::Value;
    use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
    use opentelemetry_proto::tonic::logs::v1::LogRecord as OtlpLogRecord;
    use opentelemetry_proto::tonic::resource::v1::Resource;
    use opentelemetry_proto::tonic::trace::v1::Span as OtlpSpan;

    use super::{decode_log, decode_span};

    #[test]
    fn decodes_log_and_service() {
        let resource = Resource {
            attributes: vec![KeyValue {
                key: "service.name".into(),
                value: Some(AnyValue {
                    value: Some(Value::StringValue("api".into())),
                }),
            }],
            dropped_attributes_count: 0,
            entity_refs: vec![],
        };

        let log = OtlpLogRecord {
            time_unix_nano: 1_700_000_000_000_000_000,
            observed_time_unix_nano: 0,
            severity_number: 17,
            severity_text: "ERROR".into(),
            body: Some(AnyValue {
                value: Some(Value::StringValue("boom".into())),
            }),
            attributes: vec![KeyValue {
                key: "peer".into(),
                value: Some(AnyValue {
                    value: Some(Value::StringValue("redis:6379".into())),
                }),
            }],
            dropped_attributes_count: 0,
            flags: 0,
            trace_id: vec![1; 16],
            span_id: vec![2; 8],
            event_name: "".into(),
        };

        let out = decode_log(Some(&resource), None, &log);
        assert_eq!(out.service, "api");
        assert_eq!(out.body, "boom");
        assert_eq!(
            out.trace_id.as_deref(),
            Some("01010101010101010101010101010101")
        );
    }

    #[test]
    fn decodes_span_defaults_status() {
        let span = OtlpSpan {
            trace_id: vec![1; 16],
            span_id: vec![2; 8],
            parent_span_id: vec![],
            name: "call".into(),
            start_time_unix_nano: 1_700_000_000_000_000_000,
            end_time_unix_nano: 1_700_000_000_100_000_000,
            status: None,
            ..Default::default()
        };

        let out = decode_span(None, &span);
        assert_eq!(out.status, "OK");
        assert_eq!(out.name, "call");
    }
}
