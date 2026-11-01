//! Shared provider-embed contract validation. No source-specific defaults.
use anm_sdk::protocol::provider_embed::{EmbedPayload, EmbedVariant};
use url::Url;

pub fn parse(value: &str) -> Result<EmbedPayload, String> {
    let payload: EmbedPayload =
        serde_json::from_str(value).map_err(|e| format!("embed payload: {e}"))?;
    if payload.variants.is_empty() || payload.variants.len() > 128 {
        return Err("embed payload must contain 1..128 dubs".into());
    }
    let mut ids = std::collections::HashSet::new();
    for variant in &payload.variants {
        if variant.id.is_empty() || variant.label.trim().is_empty() || !ids.insert(&variant.id) {
            return Err("embed dub ids must be unique and labels non-empty".into());
        }
        https_url(&variant.url)?;
    }
    if let Some(referer) = &payload.referer {
        https_url(referer)?;
    }
    Ok(payload)
}

pub fn https_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|e| format!("embed url: {e}"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("embed requires an absolute HTTPS URL without credentials".into());
    }
    Ok(url)
}

pub fn selected<'a>(payload: &'a EmbedPayload, id: &str) -> Result<&'a EmbedVariant, String> {
    payload
        .variants
        .iter()
        .find(|variant| variant.id == id)
        .ok_or_else(|| "unknown dub id".into())
}

/// Playback requests are never cached: signatures and resolved URLs are ephemeral.
pub fn request(url: &str, referer: Option<&str>, body: Option<Vec<u8>>) -> Result<String, String> {
    let origin = https_url(url)?.origin().ascii_serialization();
    let mut headers = vec![(
        "user-agent".into(),
        "AniMex/0.1 (+https://animex.local)".into(),
    )];
    if let Some(referer) = referer {
        headers.push(("referer".into(), referer.into()));
    }
    if body.is_some() {
        headers.extend([
            (
                "content-type".into(),
                "application/x-www-form-urlencoded".into(),
            ),
            ("origin".into(), origin),
            ("x-requested-with".into(), "XMLHttpRequest".into()),
        ]);
    }
    let response = anm_sdk::http_request(&anm_sdk::HttpRequest {
        url: url.into(),
        method: body.as_ref().map(|_| "POST".into()),
        headers,
        body,
    })?;
    if !(200..300).contains(&response.status) {
        return Err(format!("provider HTTP {}", response.status));
    }
    String::from_utf8(response.body).map_err(|_| "provider returned invalid text".into())
}

pub fn attribute(body: &str, name: &str) -> Option<String> {
    let start = body.find(name)? + name.len();
    let rest = body[start..].trim_start().strip_prefix('=')?.trim_start();
    let quote = rest.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    Some(rest[1..].split(quote).next()?.to_string())
}

pub fn unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#038;", "&")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_urls_and_identity() {
        for url in [
            "javascript:alert(1)",
            "file:///a",
            "http://example.org/v",
            "https://user:pass@example.org/v",
        ] {
            assert!(https_url(url).is_err());
        }
        assert!(https_url("https://another-source.example/embed/123").is_ok());
        assert!(parse(r#"{"variants":[]}"#).is_err());
        assert!(parse(r#"{"variants":[{"id":"a","label":"Dub","url":"https://x/v"},{"id":"a","label":"Other","url":"https://x/v"}]}"#).is_err());
    }
}
