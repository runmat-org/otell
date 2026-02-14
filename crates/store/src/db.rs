use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, NaiveDateTime, Utc};
use duckdb::Connection;
use otell_core::error::{OtellError, Result};
use otell_core::model::log::LogRecord;
use otell_core::query::StatusResponse;
use tokio::sync::broadcast;

use crate::schema::SCHEMA_SQL;

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
    db_path: String,
    log_tx: broadcast::Sender<LogRecord>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| OtellError::Io(format!("failed to create db dir: {e}")))?;
        }

        let conn = Connection::open(path)
            .map_err(|e| OtellError::Store(format!("failed to open duckdb: {e}")))?;
        conn.execute_batch("PRAGMA threads=4;")
            .map_err(|e| OtellError::Store(format!("failed to set pragmas: {e}")))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| OtellError::Store(format!("failed to initialize schema: {e}")))?;

        let (log_tx, _) = broadcast::channel(8192);

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: path.display().to_string(),
            log_tx,
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| OtellError::Store(format!("failed to open in-memory db: {e}")))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| OtellError::Store(format!("failed to initialize schema: {e}")))?;
        let (log_tx, _) = broadcast::channel(8192);
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: ":memory:".to_string(),
            log_tx,
        })
    }

    pub(crate) fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("store mutex poisoned")
    }

    pub fn status(&self) -> Result<StatusResponse> {
        let conn = self.conn();

        let logs_count = scalar_usize(&conn, "SELECT COUNT(*) FROM logs")?;
        let spans_count = scalar_usize(&conn, "SELECT COUNT(*) FROM spans")?;
        let metrics_count = scalar_usize(&conn, "SELECT COUNT(*) FROM metric_points")?;

        let oldest_ts = scalar_ts(&conn, "SELECT MIN(ts) FROM logs")?;
        let newest_ts = scalar_ts(&conn, "SELECT MAX(ts) FROM logs")?;

        let db_size_bytes = if self.db_path == ":memory:" {
            0
        } else {
            fs::metadata(&self.db_path).map(|m| m.len()).unwrap_or(0)
        };

        Ok(StatusResponse {
            db_path: self.db_path.clone(),
            db_size_bytes,
            logs_count,
            spans_count,
            metrics_count,
            oldest_ts,
            newest_ts,
        })
    }

    pub fn subscribe_logs(&self) -> broadcast::Receiver<LogRecord> {
        self.log_tx.subscribe()
    }

    pub(crate) fn publish_log(&self, record: LogRecord) {
        let _ = self.log_tx.send(record);
    }
}

fn scalar_usize(conn: &Connection, sql: &str) -> Result<usize> {
    conn.query_row(sql, [], |row| row.get::<_, i64>(0))
        .map(|v| v as usize)
        .map_err(|e| OtellError::Store(format!("query failed: {e}")))
}

fn scalar_ts(conn: &Connection, sql: &str) -> Result<Option<DateTime<Utc>>> {
    conn.query_row(sql, [], |row| row.get::<_, Option<NaiveDateTime>>(0))
        .map(|opt| opt.map(|dt| dt.and_utc()))
        .map_err(|e| OtellError::Store(format!("query failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_initializes() {
        let store = Store::open_in_memory().unwrap();
        let status = store.status().unwrap();
        assert_eq!(status.logs_count, 0);
        assert_eq!(status.spans_count, 0);
        assert_eq!(status.metrics_count, 0);
    }
}
