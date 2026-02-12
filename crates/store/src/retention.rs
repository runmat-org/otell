use std::fs;
use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use duckdb::params;
use otell_core::error::{OtellError, Result};

use crate::Store;

impl Store {
    pub fn run_retention(&self, ttl: Duration, max_bytes: u64) -> Result<()> {
        self.prune_ttl(ttl)?;
        self.prune_size(max_bytes)?;
        Ok(())
    }

    pub fn prune_ttl(&self, ttl: Duration) -> Result<()> {
        let cutoff = Utc::now()
            - chrono::Duration::from_std(ttl)
                .map_err(|e| OtellError::Internal(format!("ttl conversion failed: {e}")))?;
        let cutoff = cutoff.to_rfc3339();

        let conn = self.conn();
        conn.execute("DELETE FROM logs WHERE ts < ?", params![cutoff.clone()])
            .map_err(|e| OtellError::Store(format!("retention logs delete failed: {e}")))?;
        conn.execute(
            "DELETE FROM spans WHERE end_ts < ?",
            params![cutoff.clone()],
        )
        .map_err(|e| OtellError::Store(format!("retention spans delete failed: {e}")))?;
        conn.execute("DELETE FROM metric_points WHERE ts < ?", params![cutoff])
            .map_err(|e| OtellError::Store(format!("retention metrics delete failed: {e}")))?;

        Ok(())
    }

    pub fn prune_size(&self, max_bytes: u64) -> Result<()> {
        let status = self.status()?;
        if status.db_path == ":memory:" {
            return Ok(());
        }

        let path = Path::new(&status.db_path);
        let size = fs::metadata(path)
            .map_err(|e| OtellError::Io(format!("failed to stat db: {e}")))?
            .len();
        if size <= max_bytes {
            return Ok(());
        }

        let conn = self.conn();
        conn.execute(
            "DELETE FROM logs WHERE id IN (SELECT id FROM logs ORDER BY ts ASC LIMIT 10000)",
            [],
        )
        .map_err(|e| OtellError::Store(format!("size prune logs failed: {e}")))?;
        conn.execute(
            "DELETE FROM metric_points WHERE id IN (SELECT id FROM metric_points ORDER BY ts ASC LIMIT 10000)",
            [],
        )
        .map_err(|e| OtellError::Store(format!("size prune metrics failed: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::TimeZone;
    use otell_core::model::log::LogRecord;

    use crate::Store;

    #[test]
    fn ttl_prunes_old_logs() {
        let store = Store::open_in_memory().unwrap();
        let old = chrono::Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        store
            .insert_logs(&[LogRecord {
                ts: old,
                service: "api".into(),
                severity: 9,
                trace_id: None,
                span_id: None,
                body: "old".into(),
                attrs_json: "{}".into(),
                attrs_text: "".into(),
            }])
            .unwrap();

        store.prune_ttl(Duration::from_secs(60)).unwrap();
        let status = store.status().unwrap();
        assert_eq!(status.logs_count, 0);
    }
}
