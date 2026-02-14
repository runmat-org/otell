use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::error::{OtellError, Result};

pub fn parse_time_or_relative(input: &str) -> Result<DateTime<Utc>> {
    if let Ok(ts) = DateTime::parse_from_rfc3339(input) {
        return Ok(ts.with_timezone(&Utc));
    }

    if let Ok(duration) = humantime::parse_duration(input) {
        return Ok(Utc::now()
            - chrono::Duration::from_std(duration).map_err(|e| {
                OtellError::Parse(format!("failed to parse duration to chrono: {e}"))
            })?);
    }

    Err(OtellError::Parse(format!(
        "expected RFC3339 time or duration, got {input}"
    )))
}

pub fn parse_duration_str(input: &str) -> Result<Duration> {
    humantime::parse_duration(input)
        .map_err(|e| OtellError::Parse(format!("invalid duration {input}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339() {
        let ts = parse_time_or_relative("2026-01-01T00:00:00Z").unwrap();
        assert_eq!(ts.to_rfc3339(), "2026-01-01T00:00:00+00:00");
    }

    #[test]
    fn parses_duration() {
        let now = Utc::now();
        let ts = parse_time_or_relative("5m").unwrap();
        assert!(ts < now);
    }

    #[test]
    fn rejects_invalid() {
        assert!(parse_time_or_relative("nope").is_err());
    }
}
