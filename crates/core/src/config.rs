use std::env;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{OtellError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub db_path: PathBuf,
    pub otlp_grpc_addr: String,
    pub otlp_http_addr: String,
    pub query_tcp_addr: String,
    pub query_http_addr: String,
    pub uds_path: PathBuf,
    pub retention_ttl: Duration,
    pub retention_max_bytes: u64,
    pub write_batch_size: usize,
    pub write_flush_ms: u64,
    pub forward_otlp_endpoint: Option<String>,
    pub forward_otlp_protocol: String,
    pub forward_otlp_compression: String,
    pub forward_otlp_headers: Vec<(String, String)>,
    pub forward_otlp_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let xdg_runtime = env::var("XDG_RUNTIME_DIR").ok();
        let data_home = env::var("XDG_DATA_HOME").ok();

        let data_root = data_home
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".local/share"));

        let uds_path = xdg_runtime
            .map(PathBuf::from)
            .unwrap_or_else(|| data_root.join("otell"))
            .join("otell.sock");

        Self {
            db_path: data_root.join("otell/otell.duckdb"),
            otlp_grpc_addr: "127.0.0.1:4317".to_string(),
            otlp_http_addr: "127.0.0.1:4318".to_string(),
            query_tcp_addr: "127.0.0.1:1777".to_string(),
            query_http_addr: "127.0.0.1:1778".to_string(),
            uds_path,
            retention_ttl: Duration::from_secs(60 * 60 * 24),
            retention_max_bytes: 2 * 1024 * 1024 * 1024,
            write_batch_size: 2048,
            write_flush_ms: 200,
            forward_otlp_endpoint: None,
            forward_otlp_protocol: "grpc".to_string(),
            forward_otlp_compression: "none".to_string(),
            forward_otlp_headers: Vec::new(),
            forward_otlp_timeout: Duration::from_secs(10),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let mut cfg = Self::default();

        if let Ok(v) = env::var("OTELL_DB_PATH") {
            cfg.db_path = PathBuf::from(v);
        }
        if let Ok(v) = env::var("OTELL_OTLP_GRPC_ADDR") {
            cfg.otlp_grpc_addr = v;
        }
        if let Ok(v) = env::var("OTELL_OTLP_HTTP_ADDR") {
            cfg.otlp_http_addr = v;
        }
        if let Ok(v) = env::var("OTELL_QUERY_TCP_ADDR") {
            cfg.query_tcp_addr = v;
        }
        if let Ok(v) = env::var("OTELL_QUERY_HTTP_ADDR") {
            cfg.query_http_addr = v;
        }
        if let Ok(v) = env::var("OTELL_QUERY_UDS_PATH") {
            cfg.uds_path = PathBuf::from(v);
        }
        if let Ok(v) = env::var("OTELL_RETENTION_TTL") {
            cfg.retention_ttl = humantime::parse_duration(&v)
                .map_err(|e| OtellError::Config(format!("bad OTELL_RETENTION_TTL: {e}")))?;
        }
        if let Ok(v) = env::var("OTELL_RETENTION_MAX_BYTES") {
            cfg.retention_max_bytes = v
                .parse::<u64>()
                .map_err(|e| OtellError::Config(format!("bad OTELL_RETENTION_MAX_BYTES: {e}")))?;
        }
        if let Ok(v) = env::var("OTELL_FORWARD_OTLP_ENDPOINT") {
            cfg.forward_otlp_endpoint = Some(v);
        }
        if let Ok(v) = env::var("OTELL_FORWARD_OTLP_PROTOCOL") {
            cfg.forward_otlp_protocol = v;
        }
        if let Ok(v) = env::var("OTELL_FORWARD_OTLP_COMPRESSION") {
            cfg.forward_otlp_compression = v;
        }
        if let Ok(v) = env::var("OTELL_FORWARD_OTLP_HEADERS") {
            cfg.forward_otlp_headers = parse_otlp_headers(&v)
                .map_err(|e| OtellError::Config(format!("bad OTELL_FORWARD_OTLP_HEADERS: {e}")))?;
        }
        if let Ok(v) = env::var("OTELL_FORWARD_OTLP_TIMEOUT") {
            cfg.forward_otlp_timeout = humantime::parse_duration(&v)
                .map_err(|e| OtellError::Config(format!("bad OTELL_FORWARD_OTLP_TIMEOUT: {e}")))?;
        }

        Ok(cfg)
    }
}

fn parse_otlp_headers(raw: &str) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for entry in raw.split(',') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(OtellError::Config(
                "header entries must use key=value syntax".to_string(),
            ));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(OtellError::Config("header key cannot be empty".to_string()));
        }
        out.push((key.to_string(), value.trim().to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_expected_ports() {
        let cfg = Config::default();
        assert_eq!(cfg.otlp_grpc_addr, "127.0.0.1:4317");
        assert_eq!(cfg.otlp_http_addr, "127.0.0.1:4318");
        assert_eq!(cfg.query_tcp_addr, "127.0.0.1:1777");
        assert_eq!(cfg.query_http_addr, "127.0.0.1:1778");
    }

    #[test]
    fn default_has_retention() {
        let cfg = Config::default();
        assert_eq!(cfg.retention_ttl, Duration::from_secs(86_400));
        assert!(cfg.retention_max_bytes > 1024 * 1024);
    }

    #[test]
    fn parse_otlp_headers_accepts_list() {
        let headers = parse_otlp_headers("x-tenant=dev,authorization=Bearer token").unwrap();
        assert_eq!(
            headers,
            vec![
                ("x-tenant".to_string(), "dev".to_string()),
                ("authorization".to_string(), "Bearer token".to_string())
            ]
        );
    }

    #[test]
    fn parse_otlp_headers_rejects_bad_entries() {
        assert!(parse_otlp_headers("x-tenant").is_err());
        assert!(parse_otlp_headers("=dev").is_err());
    }
}
