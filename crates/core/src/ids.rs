use serde::{Deserialize, Serialize};

use crate::error::{OtellError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(String);

impl TraceId {
    pub fn parse(input: &str) -> Result<Self> {
        if input.len() != 32 || !input.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(OtellError::Parse(format!("invalid trace id: {input}")));
        }
        Ok(Self(input.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SpanId {
    pub fn parse(input: &str) -> Result<Self> {
        if input.len() != 16 || !input.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(OtellError::Parse(format!("invalid span id: {input}")));
        }
        Ok(Self(input.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ids() {
        let trace = TraceId::parse("4bf92f3577b34da6a3ce929d0e0e4736").unwrap();
        let span = SpanId::parse("00f067aa0ba902b7").unwrap();
        assert_eq!(trace.as_str(), "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(span.as_str(), "00f067aa0ba902b7");
    }

    #[test]
    fn rejects_bad_ids() {
        assert!(TraceId::parse("abc").is_err());
        assert!(SpanId::parse("zzzzzzzzzzzzzzzz").is_err());
    }
}
