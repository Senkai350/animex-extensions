//! JSON cache helpers over host `cache-get` / `cache-set`.

use serde::{de::DeserializeOwned, Serialize};

use crate::host;

/// Read a JSON value from the host cache. `Ok(None)` when the key is absent;
/// `Err` when the stored value does not decode into `T`.
pub fn cache_get<T: DeserializeOwned>(key: &str) -> Result<Option<T>, String> {
    match host::cache_get(key) {
        Some(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("cache decode {key}: {e}")),
        None => Ok(None),
    }
}

/// Store a JSON value in the host cache (the host applies storage policy and
/// size limits).
pub fn cache_set<T: Serialize>(key: &str, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| format!("cache encode {key}: {e}"))?;
    host::cache_set(key, &bytes);
    Ok(())
}

/// Whether the host requires fresh metadata for this scoped module call.
/// Pacing, negative caches and circuit breakers must still be honored.
pub fn force_refresh() -> bool {
    #[cfg(target_arch = "wasm32")]
    { crate::config_get("cache.force_refresh").ok().flatten().as_deref() == Some("true") }
    #[cfg(not(target_arch = "wasm32"))]
    { false }
}

/// Read a successful data cache, bypassing it during a host metadata refresh.
/// Use `cache_get` for pacing, negative responses and circuit breaker state.
pub fn data_cache_get<T: DeserializeOwned>(key: &str) -> Result<Option<T>, String> {
    if force_refresh() { return Ok(None); }
    cache_get(key)
}
