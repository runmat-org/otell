use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use opentelemetry_proto::tonic::common::v1::{AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;
use prost::Message;
use serial_test::serial;

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_otell")
}

fn spawn_server(temp: &Path) -> (Child, u16, u16, u16) {
    let grpc_port = free_port();
    let http_port = free_port();
    let query_port = free_port();

    let db_path = temp.join("otell-stress.duckdb");
    let uds_path = temp.join("otell-stress.sock");

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
        .arg(format!("127.0.0.1:{}", free_port()))
        .arg("--query-uds-path")
        .arg(&uds_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn otell run");

    (child, grpc_port, http_port, query_port)
}

fn build_logs_request(batch: usize, per_batch: usize) -> ExportLogsServiceRequest {
    let base_nanos = 1_700_000_000_000_000_000u64 + (batch as u64 * 1_000_000);
    let records = (0..per_batch)
        .map(|i| LogRecord {
            time_unix_nano: base_nanos + i as u64,
            observed_time_unix_nano: 0,
            severity_number: 9,
            severity_text: "INFO".to_string(),
            body: Some(AnyValue {
                value: Some(Value::StringValue(format!(
                    "loadtest batch={batch} idx={i}"
                ))),
            }),
            attributes: vec![],
            dropped_attributes_count: 0,
            flags: 0,
            trace_id: vec![0; 16],
            span_id: vec![0; 8],
            event_name: String::new(),
        })
        .collect::<Vec<_>>();

    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "service.name".to_string(),
                    value: Some(AnyValue {
                        value: Some(Value::StringValue("stress".to_string())),
                    }),
                }],
                dropped_attributes_count: 0,
                entity_refs: vec![],
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "stress".to_string(),
                    version: "1".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                log_records: records,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

async fn wait_http_ready(http_port: u16, child: &mut Child) {
    let client = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "otell run exited before ready"
        );
        if client
            .post(format!("http://127.0.0.1:{http_port}/v1/logs"))
            .body(Vec::<u8>::new())
            .send()
            .await
            .is_ok()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for ingest HTTP"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

#[tokio::test]
#[serial]
#[ignore = "stress test; run manually"]
async fn stress_ingest_logs_and_query_count() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (mut child, _grpc_port, http_port, query_port) = spawn_server(temp.path());
    wait_http_ready(http_port, &mut child).await;

    const BATCHES: usize = 80;
    const PER_BATCH: usize = 150;
    let expected = BATCHES * PER_BATCH;

    let client = reqwest::Client::new();
    let mut tasks = tokio::task::JoinSet::new();
    let start = Instant::now();
    for batch in 0..BATCHES {
        let client = client.clone();
        tasks.spawn(async move {
            let req = build_logs_request(batch, PER_BATCH);
            let mut payload = Vec::new();
            req.encode(&mut payload).expect("encode logs req");
            let resp = client
                .post(format!("http://127.0.0.1:{http_port}/v1/logs"))
                .body(payload)
                .send()
                .await
                .expect("post logs");
            assert!(resp.status().is_success(), "ingest request failed");
        });
    }
    while let Some(joined) = tasks.join_next().await {
        joined.expect("join ingest task");
    }

    let ingest_elapsed = start.elapsed();

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let status_out = Command::new(bin())
            .arg("--json")
            .arg("status")
            .arg("--addr")
            .arg(format!("127.0.0.1:{query_port}"))
            .output()
            .expect("status output");
        let value: serde_json::Value =
            serde_json::from_slice(&status_out.stdout).expect("status json parse");
        let logs_count = value["Status"]["logs_count"].as_u64().unwrap_or(0) as usize;

        if logs_count >= expected {
            break;
        }
        assert!(Instant::now() < deadline, "timed out waiting for writes");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let search_out = Command::new(bin())
        .arg("search")
        .arg("loadtest")
        .arg("--count")
        .arg("--addr")
        .arg(format!("127.0.0.1:{query_port}"))
        .output()
        .expect("search output");
    let stdout = String::from_utf8_lossy(&search_out.stdout);
    assert!(stdout.contains(&format!("-- {expected} matches (0 returned) --")));

    let _ = child.kill();
    let _ = child.wait();

    eprintln!("stress complete: {} logs in {:?}", expected, ingest_elapsed);
}
