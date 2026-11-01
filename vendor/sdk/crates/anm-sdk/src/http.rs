//! HTTP helpers over the host allowlisted client.

use serde::{de::DeserializeOwned, Serialize};

use crate::host;

/// Host request type (`http-request-params`).
pub type HttpRequest = host::HttpRequestParams;
/// Host response type (`http-response`).
pub type HttpResponse = host::HttpResponse;

/// Low-level HTTP request through the host. The host enforces the domain
/// allowlist, redirect policy, timeout and response size.
pub fn http_request(req: &HttpRequest) -> Result<HttpResponse, String> {
    host::http_request(req)
}

/// GET without a body or extra headers.
pub fn get(url: &str) -> Result<HttpResponse, String> {
    http_request(&HttpRequest {
        url: url.to_string(),
        method: None,
        headers: Vec::new(),
        body: None,
    })
}

/// Reject non-2xx responses with the URL in the message.
pub fn ensure_success(resp: &HttpResponse, url: &str) -> Result<(), String> {
    if (200..300).contains(&resp.status) {
        Ok(())
    } else {
        Err(format!("http status {} for {url}", resp.status))
    }
}

/// GET + JSON decode.
pub fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, String> {
    let resp = get(url)?;
    ensure_success(&resp, url)?;
    serde_json::from_slice(&resp.body).map_err(|e| format!("json decode {url}: {e}"))
}

/// POST a JSON body (`Content-Type: application/json`) + JSON decode.
pub fn post_json<B: Serialize, T: DeserializeOwned>(url: &str, body: &B) -> Result<T, String> {
    let payload = serde_json::to_vec(body).map_err(|e| format!("json encode: {e}"))?;
    let resp = http_request(&HttpRequest {
        url: url.to_string(),
        method: Some("POST".to_string()),
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: Some(payload),
    })?;
    ensure_success(&resp, url)?;
    serde_json::from_slice(&resp.body).map_err(|e| format!("json decode {url}: {e}"))
}
