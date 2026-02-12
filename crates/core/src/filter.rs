use std::str::FromStr;

use chrono::{DateTime, Utc};
use glob::Pattern;
use serde::{Deserialize, Serialize};

use crate::error::{OtellError, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Trace = 1,
    Debug = 5,
    Info = 9,
    Warn = 13,
    Error = 17,
    Fatal = 21,
}

impl FromStr for Severity {
    type Err = OtellError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "TRACE" => Ok(Self::Trace),
            "DEBUG" => Ok(Self::Debug),
            "INFO" => Ok(Self::Info),
            "WARN" | "WARNING" => Ok(Self::Warn),
            "ERROR" => Ok(Self::Error),
            "FATAL" => Ok(Self::Fatal),
            _ => Err(OtellError::Parse(format!("unknown severity: {s}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    TsAsc,
    TsDesc,
    DurationDesc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttrFilter {
    pub key: String,
    pub value_glob: String,
}

impl AttrFilter {
    pub fn parse(input: &str) -> Result<Self> {
        let (key, value_glob) = input
            .split_once('=')
            .ok_or_else(|| OtellError::Parse(format!("invalid where filter: {input}")))?;

        if key.trim().is_empty() || value_glob.trim().is_empty() {
            return Err(OtellError::Parse(format!("invalid where filter: {input}")));
        }

        Ok(Self {
            key: key.trim().to_string(),
            value_glob: value_glob.trim().to_string(),
        })
    }

    pub fn matches(&self, value: &str) -> bool {
        Pattern::new(&self.value_glob)
            .map(|p| p.matches(value))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindow {
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

impl TimeWindow {
    pub fn all() -> Self {
        Self {
            since: None,
            until: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_parse() {
        assert_eq!(Severity::from_str("warn").unwrap(), Severity::Warn);
        assert!(Severity::from_str("wat").is_err());
    }

    #[test]
    fn attr_filter_parse_and_match() {
        let f = AttrFilter::parse("attrs.peer=redis:*").unwrap();
        assert_eq!(f.key, "attrs.peer");
        assert!(f.matches("redis:6379"));
        assert!(!f.matches("postgres:5432"));
    }
}
