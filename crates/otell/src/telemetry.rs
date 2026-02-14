use std::collections::HashMap;
use std::io::IsTerminal;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Utc;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace as sdktrace;
use otell_core::model::log::LogRecord;
use otell_core::model::span::SpanRecord;
use otell_store::Store;
use tokio::sync::mpsc;
use tracing::{Event, Id, Subscriber};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub self_observe: SelfObserveMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfObserveMode {
    Off,
    Store,
    Both,
}

impl SelfObserveMode {
    pub fn from_env() -> Self {
        match std::env::var("OTELL_SELF_OBSERVE")
            .unwrap_or_else(|_| "off".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "1" | "true" | "yes" | "on" | "store" => Self::Store,
            "both" => Self::Both,
            _ => Self::Off,
        }
    }

    pub fn uses_store(self) -> bool {
        matches!(self, Self::Store | Self::Both)
    }
}

pub fn init_cli_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .compact()
        .try_init();
}

pub fn init_run_tracing(cfg: TelemetryConfig, store: Option<Store>) {
    let env_filter = EnvFilter::from_default_env();
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .compact();

    let otlp_layer = build_otlp_layer();
    let store_layer = if cfg.self_observe.uses_store() {
        store.map(SelfObserveLayer::new)
    } else {
        None
    };

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(otlp_layer)
        .with(store_layer)
        .try_init();
}

pub fn shutdown_tracing() {
    if let Some(provider) = otlp_provider_slot()
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
    {
        let _ = provider.shutdown();
    }
}

fn build_otlp_layer<S>() -> Option<OpenTelemetryLayer<S, sdktrace::Tracer>>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let has_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok();
    if !has_endpoint {
        return None;
    }

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
        .ok()?;

    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();
    let tracer = provider.tracer("otell");

    if let Ok(mut slot) = otlp_provider_slot().lock() {
        *slot = Some(provider);
    }

    Some(tracing_opentelemetry::layer().with_tracer(tracer))
}

fn otlp_provider_slot() -> &'static Mutex<Option<sdktrace::SdkTracerProvider>> {
    static SLOT: OnceLock<Mutex<Option<sdktrace::SdkTracerProvider>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone)]
enum Signal {
    Log(LogRecord),
    Span(SpanRecord),
}

#[derive(Debug, Clone)]
struct SpanStart {
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
    name: String,
    start_ts: chrono::DateTime<Utc>,
}

#[derive(Clone)]
struct SelfObserveLayer {
    tx: mpsc::UnboundedSender<Signal>,
    spans: Arc<Mutex<HashMap<u64, SpanStart>>>,
}

impl SelfObserveLayer {
    fn new(store: Store) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Signal>();
        tokio::spawn(async move {
            let mut logs = Vec::new();
            let mut spans = Vec::new();
            while let Some(signal) = rx.recv().await {
                match signal {
                    Signal::Log(log) => logs.push(log),
                    Signal::Span(span) => spans.push(span),
                }

                if logs.len() >= 256 {
                    let _ = store.insert_logs(&logs);
                    logs.clear();
                }
                if spans.len() >= 128 {
                    let _ = store.insert_spans(&spans);
                    spans.clear();
                }
            }

            if !logs.is_empty() {
                let _ = store.insert_logs(&logs);
            }
            if !spans.is_empty() {
                let _ = store.insert_spans(&spans);
            }
        });

        Self {
            tx,
            spans: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<S> Layer<S> for SelfObserveLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if event.metadata().target().starts_with("otell::telemetry") {
            return;
        }

        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        let level = match *event.metadata().level() {
            tracing::Level::TRACE => 1,
            tracing::Level::DEBUG => 5,
            tracing::Level::INFO => 9,
            tracing::Level::WARN => 13,
            tracing::Level::ERROR => 17,
        };

        let mut trace_id = None;
        let mut span_id = None;
        if let Some(current) = ctx.lookup_current() {
            let id = current.id().into_u64();
            if let Some(span) = self.spans.lock().ok().and_then(|m| m.get(&id).cloned()) {
                trace_id = Some(span.trace_id);
                span_id = Some(span.span_id);
            }
        }

        let attrs_json =
            serde_json::to_string(&visitor.fields).unwrap_or_else(|_| "{}".to_string());
        let attrs_text = visitor
            .fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ");

        let body = visitor
            .message
            .unwrap_or_else(|| event.metadata().name().to_string());

        let _ = self.tx.send(Signal::Log(LogRecord {
            ts: Utc::now(),
            service: "otell".to_string(),
            severity: level,
            trace_id,
            span_id,
            body,
            attrs_json,
            attrs_text,
        }));
    }

    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let id_u64 = id.into_u64();
        let span_id = format!("{id_u64:016x}");

        let parent_id = attrs
            .parent()
            .map(Id::into_u64)
            .or_else(|| ctx.lookup_current().map(|s| s.id().into_u64()));

        let (trace_id, parent_span_id) = if let Some(pid) = parent_id {
            if let Some(parent) = self.spans.lock().ok().and_then(|m| m.get(&pid).cloned()) {
                (parent.trace_id, Some(parent.span_id))
            } else {
                (uuid::Uuid::new_v4().simple().to_string(), None)
            }
        } else {
            (uuid::Uuid::new_v4().simple().to_string(), None)
        };

        let name = attrs.metadata().name().to_string();
        let start = SpanStart {
            trace_id,
            span_id,
            parent_span_id,
            name,
            start_ts: Utc::now(),
        };

        if let Ok(mut map) = self.spans.lock() {
            map.insert(id_u64, start);
        }
    }

    fn on_close(&self, id: Id, _ctx: Context<'_, S>) {
        let Some(start) = self
            .spans
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&id.into_u64()))
        else {
            return;
        };

        let _ = self.tx.send(Signal::Span(SpanRecord {
            trace_id: start.trace_id,
            span_id: start.span_id,
            parent_span_id: start.parent_span_id,
            service: "otell".to_string(),
            name: start.name,
            start_ts: start.start_ts,
            end_ts: Utc::now(),
            status: "OK".to_string(),
            attrs_json: "{}".to_string(),
            events_json: "[]".to_string(),
        }));
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: Option<String>,
    fields: HashMap<String, String>,
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(rendered.trim_matches('"').to_string());
        }
        self.fields.insert(field.name().to_string(), rendered);
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}
