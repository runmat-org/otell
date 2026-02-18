use std::env;
use std::fs;
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
    pub fn load() -> Result<Self> {
        let mut cfg = Self::default();
        let config_path = config_file_path();
        if let Some(file_overrides) = load_file_overrides(&config_path)? {
            apply_overrides(&mut cfg, file_overrides, "config file")?;
        }
        let env_overrides = load_env_overrides()?;
        apply_overrides(&mut cfg, env_overrides, "environment")?;
        Ok(cfg)
    }

    pub fn from_env() -> Result<Self> {
        let mut cfg = Self::default();
        let env_overrides = load_env_overrides()?;
        apply_overrides(&mut cfg, env_overrides, "environment")?;
        Ok(cfg)
    }
}

#[derive(Debug, Default, Deserialize)]
struct ConfigOverrides {
    db_path: Option<PathBuf>,
    otlp_grpc_addr: Option<String>,
    otlp_http_addr: Option<String>,
    query_tcp_addr: Option<String>,
    query_http_addr: Option<String>,
    uds_path: Option<PathBuf>,
    retention_ttl: Option<String>,
    retention_max_bytes: Option<u64>,
    write_batch_size: Option<usize>,
    write_flush_ms: Option<u64>,
    forward_otlp_endpoint: Option<String>,
    forward_otlp_protocol: Option<String>,
    forward_otlp_compression: Option<String>,
    forward_otlp_headers: Option<String>,
    forward_otlp_timeout: Option<String>,
}

fn config_file_path() -> PathBuf {
    if let Ok(path) = env::var("OTELL_CONFIG") {
        return PathBuf::from(path);
    }

    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let config_home = env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(home).join(".config"));
    config_home.join("otell/config.toml")
}

fn load_file_overrides(path: &PathBuf) -> Result<Option<ConfigOverrides>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)
        .map_err(|e| OtellError::Config(format!("failed reading {}: {e}", path.display())))?;
    let parsed: ConfigOverrides = toml::from_str(&raw)
        .map_err(|e| OtellError::Config(format!("failed parsing {}: {e}", path.display())))?;
    Ok(Some(parsed))
}

fn load_env_overrides() -> Result<ConfigOverrides> {
    let retention_max_bytes = match env::var("OTELL_RETENTION_MAX_BYTES") {
        Ok(v) => Some(v.parse::<u64>().map_err(|e| {
            OtellError::Config(format!("bad OTELL_RETENTION_MAX_BYTES in environment: {e}"))
        })?),
        Err(_) => None,
    };

    Ok(ConfigOverrides {
        db_path: env::var("OTELL_DB_PATH").ok().map(PathBuf::from),
        otlp_grpc_addr: env::var("OTELL_OTLP_GRPC_ADDR").ok(),
        otlp_http_addr: env::var("OTELL_OTLP_HTTP_ADDR").ok(),
        query_tcp_addr: env::var("OTELL_QUERY_TCP_ADDR").ok(),
        query_http_addr: env::var("OTELL_QUERY_HTTP_ADDR").ok(),
        uds_path: env::var("OTELL_QUERY_UDS_PATH").ok().map(PathBuf::from),
        retention_ttl: env::var("OTELL_RETENTION_TTL").ok(),
        retention_max_bytes,
        write_batch_size: None,
        write_flush_ms: None,
        forward_otlp_endpoint: env::var("OTELL_FORWARD_OTLP_ENDPOINT").ok(),
        forward_otlp_protocol: env::var("OTELL_FORWARD_OTLP_PROTOCOL").ok(),
        forward_otlp_compression: env::var("OTELL_FORWARD_OTLP_COMPRESSION").ok(),
        forward_otlp_headers: env::var("OTELL_FORWARD_OTLP_HEADERS").ok(),
        forward_otlp_timeout: env::var("OTELL_FORWARD_OTLP_TIMEOUT").ok(),
    })
}

fn apply_overrides(cfg: &mut Config, overrides: ConfigOverrides, source: &str) -> Result<()> {
    if let Some(v) = overrides.db_path {
        cfg.db_path = v;
    }
    if let Some(v) = overrides.otlp_grpc_addr {
        cfg.otlp_grpc_addr = v;
    }
    if let Some(v) = overrides.otlp_http_addr {
        cfg.otlp_http_addr = v;
    }
    if let Some(v) = overrides.query_tcp_addr {
        cfg.query_tcp_addr = v;
    }
    if let Some(v) = overrides.query_http_addr {
        cfg.query_http_addr = v;
    }
    if let Some(v) = overrides.uds_path {
        cfg.uds_path = v;
    }
    if let Some(v) = overrides.retention_ttl {
        cfg.retention_ttl = humantime::parse_duration(&v).map_err(|e| {
            OtellError::Config(format!("bad retention_ttl in {source}: {e} (value={v})"))
        })?;
    }
    if let Some(v) = overrides.retention_max_bytes {
        cfg.retention_max_bytes = v;
    }
    if let Some(v) = overrides.write_batch_size {
        cfg.write_batch_size = v;
    }
    if let Some(v) = overrides.write_flush_ms {
        cfg.write_flush_ms = v;
    }
    if let Some(v) = overrides.forward_otlp_endpoint {
        cfg.forward_otlp_endpoint = Some(v);
    }
    if let Some(v) = overrides.forward_otlp_protocol {
        cfg.forward_otlp_protocol = v;
    }
    if let Some(v) = overrides.forward_otlp_compression {
        cfg.forward_otlp_compression = v;
    }
    if let Some(v) = overrides.forward_otlp_headers {
        cfg.forward_otlp_headers = parse_otlp_headers(&v).map_err(|e| {
            OtellError::Config(format!(
                "bad forward_otlp_headers in {source}: {e} (value={v})"
            ))
        })?;
    }
    if let Some(v) = overrides.forward_otlp_timeout {
        cfg.forward_otlp_timeout = humantime::parse_duration(&v).map_err(|e| {
            OtellError::Config(format!(
                "bad forward_otlp_timeout in {source}: {e} (value={v})"
            ))
        })?;
    }
    Ok(())
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

    #[test]
    fn apply_file_overrides_updates_forwarding_fields() {
        let mut cfg = Config::default();
        let file = ConfigOverrides {
            forward_otlp_endpoint: Some("http://127.0.0.1:4317".to_string()),
            forward_otlp_protocol: Some("http/protobuf".to_string()),
            forward_otlp_compression: Some("gzip".to_string()),
            forward_otlp_headers: Some("x-tenant=dev,authorization=Bearer token".to_string()),
            forward_otlp_timeout: Some("3s".to_string()),
            ..ConfigOverrides::default()
        };

        apply_overrides(&mut cfg, file, "config file").unwrap();

        assert_eq!(
            cfg.forward_otlp_endpoint,
            Some("http://127.0.0.1:4317".to_string())
        );
        assert_eq!(cfg.forward_otlp_protocol, "http/protobuf");
        assert_eq!(cfg.forward_otlp_compression, "gzip");
        assert_eq!(
            cfg.forward_otlp_headers,
            vec![
                ("x-tenant".to_string(), "dev".to_string()),
                ("authorization".to_string(), "Bearer token".to_string())
            ]
        );
        assert_eq!(cfg.forward_otlp_timeout, Duration::from_secs(3));
    }
}
