//! SDK for AniMex module authors: host bindings (`wit/animex.wit`, world
//! `host-imports`) plus HTTP/log/cache/config helpers and protocol types.
//!
//! A module declares `use anm_sdk::bindings;` and implements its world; host
//! functions are called through the helpers in this crate. The crate targets
//! `wasm32-wasip2`.

pub mod bindings;
mod cache;
mod http;
pub mod protocol;
mod release;

pub use bindings::animex::module::host;
pub use cache::{cache_get, cache_set, data_cache_get, force_refresh};
pub use http::{ensure_success, get, get_json, http_request, post_json, HttpRequest, HttpResponse};

/// Re-export `wit_bindgen` so modules do not pin their own copy.
pub use wit_bindgen;

/// Log an `info` message (host writes it to stderr and the plugin buffer).
pub fn log_info(message: &str) {
    host::log("info", message);
}

/// Log a `warn` message.
pub fn log_warn(message: &str) {
    host::log("warn", message);
}

/// Log an `error` message.
pub fn log_error(message: &str) {
    host::log("error", message);
}

/// Read a plugin setting supplied by the host at load time.
pub fn config_get(key: &str) -> Result<Option<String>, String> {
    Ok(host::config_get(key))
}
