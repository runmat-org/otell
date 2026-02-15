use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use opentelemetry_proto::tonic::common::v1::{AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;
use prost::Message;
use serial_test::serial;

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_otell")
}

fn spawn_server(temp: &Path) -> (Child, u16, u16, u16, u16, PathBuf, PathBuf) {
    let grpc_port = free_port();
    let http_port = free_port();
    let query_port = free_port();
    let query_http_port = free_port();
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
        .arg("--query-http-addr")
        .arg(format!("127.0.0.1:{query_http_port}"))
        .arg("--query-uds-path")
        .arg(&uds_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    (
        child,
        grpc_port,
        http_port,
        query_port,
        query_http_port,
        db_path,
        uds_path,
    )
}

fn sample_logs_request(body: &str) -> ExportLogsServiceRequest {
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
                        value: Some(Value::StringValue(body.into())),
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

async fn wait_http_ready(port: u16, child: &mut Child) {
    let client = reqwest::Client::new();
    let mut ready = false;
    for _ in 0..100 {
        assert!(child.try_wait().unwrap().is_none(), "otell exited early");
        if client
            .post(format!("http://127.0.0.1:{port}/v1/logs"))
            .body(Vec::<u8>::new())
            .send()
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(ready, "ingest endpoint not ready");
}

#[tokio::test]
#[serial]
async fn e2e_http_ingest_and_tcp_search() {
    let temp = tempfile::tempdir().unwrap();
    let (mut child, _grpc_port, http_port, query_port, _query_http_port, _db, _uds) =
        spawn_server(temp.path());

    wait_http_ready(http_port, &mut child).await;

    let req = sample_logs_request("timeout error");
    let mut payload = Vec::new();
    req.encode(&mut payload).unwrap();

    let resp = reqwest::Client::new()
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
    assert!(stdout.contains("-- 1 matches"));
    assert!(stdout.contains("handle="));

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
#[serial]
async fn e2e_search_count_stats_and_status_json_shape() {
    let temp = tempfile::tempdir().unwrap();
    let (mut child, _grpc_port, http_port, query_port, _query_http_port, _db, _uds) =
        spawn_server(temp.path());

    wait_http_ready(http_port, &mut child).await;

    let req = sample_logs_request("count me");
    let mut payload = Vec::new();
    req.encode(&mut payload).unwrap();
    reqwest::Client::new()
        .post(format!("http://127.0.0.1:{http_port}/v1/logs"))
        .body(payload)
        .send()
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    let search = Command::new(bin())
        .arg("search")
        .arg("count")
        .arg("--count")
        .arg("--stats")
        .arg("--addr")
        .arg(format!("127.0.0.1:{query_port}"))
        .output()
        .unwrap();
    let search_out = String::from_utf8_lossy(&search.stdout);
    assert!(search_out.contains("-- 1 matches (0 returned) --"));
    assert!(search_out.contains("stats.by_service"));

    let status = Command::new(bin())
        .arg("--json")
        .arg("status")
        .arg("--addr")
        .arg(format!("127.0.0.1:{query_port}"))
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert!(value.get("Status").is_some());

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
#[serial]
async fn e2e_grpc_ingest_and_uds_search() {
    let temp = tempfile::tempdir().unwrap();
    let (mut child, grpc_port, http_port, _query_port, _query_http_port, _db, uds) =
        spawn_server(temp.path());
    let _ = http_port;
    let mut exported = false;
    for _ in 0..120 {
        assert!(child.try_wait().unwrap().is_none(), "otell exited early");
        let endpoint = format!("http://127.0.0.1:{grpc_port}");
        if let Ok(mut grpc_client) = LogsServiceClient::connect(endpoint).await
            && grpc_client
                .export(tonic::Request::new(sample_logs_request("grpc path")))
                .await
                .is_ok()
        {
            exported = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(exported, "failed to export gRPC logs to otell");

    tokio::time::sleep(Duration::from_millis(300)).await;

    let output = Command::new(bin())
        .arg("search")
        .arg("grpc")
        .arg("--uds")
        .arg(uds)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("grpc path"));

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
#[serial]
async fn e2e_http_query_api() {
    let temp = tempfile::tempdir().unwrap();
    let (mut child, _grpc_port, http_port, _query_port, query_http_port, _db, _uds) =
        spawn_server(temp.path());
    wait_http_ready(http_port, &mut child).await;

    let req = sample_logs_request("via query http");
    let mut payload = Vec::new();
    req.encode(&mut payload).unwrap();
    reqwest::Client::new()
        .post(format!("http://127.0.0.1:{http_port}/v1/logs"))
        .body(payload)
        .send()
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    let body = serde_json::json!({
        "pattern": "query",
        "fixed": false,
        "ignore_case": false,
        "service": null,
        "trace_id": null,
        "span_id": null,
        "severity_gte": null,
        "attr_filters": [],
        "window": {"since": null, "until": null},
        "sort": "TsAsc",
        "limit": 100,
        "context_lines": 0,
        "context_seconds": null,
        "count_only": false,
        "include_stats": false
    });

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{query_http_port}/v1/search"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(resp.contains("via query http"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
#[serial]
fn mcp_initialize_and_tools_list() {
    let output = Command::new(bin())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            let stdin = child.stdin.as_mut().unwrap();
            stdin.write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
            )?;
            stdin.write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n",
            )?;
            drop(child.stdin.take());
            child.wait_with_output()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("protocolVersion"));
    assert!(stdout.contains("tools"));
}

#[test]
#[serial]
fn mcp_rejects_legacy_tool_shape() {
    let output = Command::new(bin())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            let stdin = child.stdin.as_mut().unwrap();
            stdin.write_all(b"{\"tool\":\"status\",\"args\":{}}\n")?;
            drop(child.stdin.take());
            child.wait_with_output()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("unsupported method"));
}

#[test]
fn intro_without_server_guides_startup() {
    let output = Command::new(bin())
        .arg("intro")
        .arg("--addr")
        .arg("127.0.0.1:1")
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("connected to running `otell run`: `false`"));
    assert!(out.contains("otell run"));
}
