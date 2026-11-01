//! Typed helpers for first-party player protocols.
//!
//! Third-party sources and players may define their own protocol id and JSON
//! schema; the host treats payloads as opaque. The types here are shared by
//! first-party modules so the wire format cannot drift.

use serde::{de::DeserializeOwned, Serialize};

pub mod anilibria_hls;
pub mod provider_embed;

/// Encode a protocol payload as compact JSON.
pub fn build_payload<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("encode protocol payload: {e}"))
}

/// Decode a protocol payload.
pub fn parse_payload<T: DeserializeOwned>(payload: &str) -> Result<T, String> {
    serde_json::from_str(payload).map_err(|e| format!("decode protocol payload: {e}"))
}
