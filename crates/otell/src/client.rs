use std::path::PathBuf;

use anyhow::Context;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UnixStream};

use crate::protocol::{ApiRequest, ApiResponse};

pub enum QueryClient {
    Uds(BufReader<UnixStream>),
    Tcp(BufReader<TcpStream>),
}

impl QueryClient {
    pub async fn connect(uds: Option<PathBuf>, addr: Option<String>) -> anyhow::Result<Self> {
        if let Some(path) = uds {
            let stream = UnixStream::connect(path)
                .await
                .context("connect UDS query server")?;
            return Ok(Self::Uds(BufReader::new(stream)));
        }

        if let Ok(path) = std::env::var("OTELL_QUERY_UDS_PATH") {
            if let Ok(stream) = UnixStream::connect(path).await {
                return Ok(Self::Uds(BufReader::new(stream)));
            }
        }

        let addr = addr
            .or_else(|| std::env::var("OTELL_QUERY_TCP_ADDR").ok())
            .unwrap_or_else(|| "127.0.0.1:1777".to_string());
        let stream = TcpStream::connect(&addr)
            .await
            .with_context(|| format!("connect query server TCP {addr}"))?;
        Ok(Self::Tcp(BufReader::new(stream)))
    }

    pub async fn request(&mut self, req: ApiRequest) -> anyhow::Result<ApiResponse> {
        let payload = serde_json::to_vec(&req)?;

        match self {
            QueryClient::Uds(stream) => {
                stream.get_mut().write_all(&payload).await?;
                stream.get_mut().write_all(b"\n").await?;
                stream.get_mut().flush().await?;

                let mut line = String::new();
                stream.read_line(&mut line).await?;
                Ok(serde_json::from_str(&line)?)
            }
            QueryClient::Tcp(stream) => {
                stream.get_mut().write_all(&payload).await?;
                stream.get_mut().write_all(b"\n").await?;
                stream.get_mut().flush().await?;

                let mut line = String::new();
                stream.read_line(&mut line).await?;
                Ok(serde_json::from_str(&line)?)
            }
        }
    }
}
