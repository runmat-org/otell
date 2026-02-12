use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use opentelemetry_proto::tonic::common::v1::{AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;
use prost::Message;

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_otell")
}

fn spawn_server(temp: &PathBuf) -> (Child, u16, u16, u16, PathBuf, PathBuf) {
    let grpc_port = free_port();
    let http_port = free_port();
    let query_port = free_port();
    let db_path = temp.join("otell.duckdb");
    let uds_path = temp.join("otell.sock");

    let child = Command::new(bin())
        .arg("run")
        .arg("--db-path")
        .arg(&db_path)
        .arg("--otlp-grpc-addr")
        .arg(format!("127.0.0.1:{grpc_port}"))
        .arg("--otlp-http-addr")
        .arg(format!("127.0.0.1:{http_port}"))
        .arg("--query-tcp-addr")
        .arg(format!("127.0.0.1:{query_port}"))
        .arg("--query-uds-path")
        .arg(&uds_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    (child, grpc_port, http_port, query_port, db_path, uds_path)
}

fn sample_logs_request() -> ExportLogsServiceRequest {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "service.name".into(),
                    value: Some(AnyValue {
                        value: Some(Value::StringValue("api".into())),
                    }),
                }],
                dropped_attributes_count: 0,
                entity_refs: vec![],
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "test".into(),
                    version: "0.1".into(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                log_records: vec![LogRecord {
                    time_unix_nano: 1_700_000_000_000_000_000,
                    observed_time_unix_nano: 0,
                    severity_number: 17,
                    severity_text: "ERROR".into(),
                    body: Some(AnyValue {
                        value: Some(Value::StringValue("timeout error".into())),
                    }),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    flags: 0,
                    trace_id: vec![1; 16],
                    span_id: vec![2; 8],
                    event_name: "".into(),
                }],
                schema_url: "".into(),
            }],
            schema_url: "".into(),
        }],
    }
}

#[tokio::test]
async fn e2e_http_ingest_and_search() {
    let temp = tempfile::tempdir().unwrap();
    let (mut child, _grpc_port, http_port, query_port, _db_path, _uds_path) =
        spawn_server(&temp.path().to_path_buf());

    let client = reqwest::Client::new();
    let mut ready = false;
    for _ in 0..20 {
        if child.try_wait().unwrap().is_some() {
            panic!("otell run exited before test query");
        }
        match client
            .post(format!("http://127.0.0.1:{http_port}/v1/logs"))
            .body(Vec::<u8>::new())
            .send()
            .await
        {
            Ok(_) => {
                ready = true;
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
    assert!(ready, "HTTP ingest endpoint did not become ready");

    let req = sample_logs_request();
    let mut payload = Vec::new();
    req.encode(&mut payload).unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{http_port}/v1/logs"))
        .body(payload)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());

    tokio::time::sleep(Duration::from_millis(300)).await;

    let output = Command::new(bin())
        .arg("search")
        .arg("timeout")
        .arg("--addr")
        .arg(format!("127.0.0.1:{query_port}"))
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("timeout error"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn doctor_prints_expected_env_hints() {
    let output = Command::new(bin()).arg("doctor").output().unwrap();
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("OTEL_EXPORTER_OTLP_ENDPOINT"));
}
