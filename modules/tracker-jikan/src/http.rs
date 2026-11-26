//! MyAnimeList HTTP client: shared pacing, conditional requests, stale cache
//! and a persisted circuit breaker.
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/Http/HttpClientWrapper.php` — every non-2xx response becomes
//!   `BadResponseException`; redirects are handled by the client.
//! - `src/Exception/BadResponseException.php`, `ParserException.php` — errors
//!   carry the request path and status instead of silently returning empty data.
//! - `src/MyAnimeList/MalClient.php` — 404 for missing titles/searches.
//!
//! References from the jikan-rest service (jikan-me/jikan-rest), which is where
//! Jikan actually solved MAL's sensitivity to scraping:
//! - `app/Services/DefaultCachedScraperService.php` — scrape only when the
//!   cached value is missing/expired; serve the previous value otherwise.
//! - `app/Support/CachedData.php`, `app/Concerns/ScraperCacheTtl.php` — TTLs.
//! - `app/Jobs/UpdateCacheJob.php` — `QUEUE_DELAY_PER_JOB=5` between refreshes.
//! - `.env.dist` — `CACHE_DEFAULT_EXPIRE=86400`, `CACHE_SEARCH_EXPIRE=432000`,
//!   `CACHE_404_EXPIRE=604800`, `SOURCE_TIMEOUT=10`.
//!
//! The Jikan PHP library itself explicitly does not rate-limit ("This library
//! does not perform any rate limitations or caching, so use it responsibly").
//! We therefore implement the service-side behaviour locally: a persisted
//! 2-second spacing between requests, long TTLs, `If-None-Match`/`If-Modified-
//! Since`, a 7-day negative cache, stale-on-error and a shared cooldown after
//! 403/429/5xx/network failures.

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anm_sdk::HttpRequest;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::html;

const SEARCH_NAMESPACE: &str = "mal:v1:search";
const PACING_KEY: &str = "mal:v1:pace";
const BREAKER_KEY: &str = "mal:v1:breaker";

/// MAL has no published API quota, but community clients and Jikan's queued
/// refreshes keep at least a few seconds between requests. Two seconds is the
/// floor; fresh cache hits never touch the network.
pub const MIN_INTERVAL_MS: u64 = 2_000;
/// Metadata changes slowly. Jikan REST uses 24 hours for titles/characters/etc.
pub const TTL_DETAIL_MS: u64 = 24 * 60 * 60 * 1_000;
/// Jikan REST keeps search responses for five days (`CACHE_SEARCH_EXPIRE`).
pub const TTL_SEARCH_MS: u64 = 5 * 24 * 60 * 60 * 1_000;
/// Characters, staff, videos, pictures, recommendations and statistics.
pub const TTL_SECTION_MS: u64 = 24 * 60 * 60 * 1_000;
/// Use a stale copy for up to this long when MAL is unavailable.
const STALE_MAX_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
/// Jikan REST caches 404s for seven days (`CACHE_404_EXPIRE`).
const MISSING_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
/// Short pause after a network/5xx failure.
const FAILURE_COOLDOWN_MS: u64 = 30_000;
/// Cloudflare/WAF rejection needs a much longer pause than a transient 5xx.
const CHALLENGE_COOLDOWN_MS: u64 = 10 * 60 * 1_000;

const USER_AGENT: &str = "AniMex/0.1 (+https://github.com/animex)";
const ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";

static LAST_REQUEST: Mutex<u64> = Mutex::new(0);
static BREAKER: Mutex<Breaker> = Mutex::new(Breaker {
    until_ms: 0,
    reason: String::new(),
});

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPage {
    saved_at: u64,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    last_modified: Option<String>,
    body: String,
}

impl CachedPage {
    fn from_response(path: &str, body: String, headers: &[(String, String)], saved_at: u64) -> Self {
        // Store the parser's slice directly so large pages fit the host cache
        // and retain their validators for conditional requests.
        let trimmed = html::trim_for(path, &body);
        let body = if trimmed.len() < body.len() {
            trimmed.to_owned()
        } else {
            body
        };
        Self {
            saved_at,
            etag: header_value(headers, "etag").map(str::to_owned),
            last_modified: header_value(headers, "last-modified").map(str::to_owned),
            body,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MissingPage {
    until_ms: u64,
    message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Breaker {
    until_ms: u64,
    #[serde(default)]
    reason: String,
}

impl Breaker {
    fn error(&self, now: u64) -> Option<String> {
        if now >= self.until_ms {
            return None;
        }
        let seconds = self.until_ms.saturating_sub(now).div_ceil(1_000);
        let reason = if self.reason.is_empty() {
            "MyAnimeList временно недоступен"
        } else {
            self.reason.as_str()
        };
        Some(format!("{reason}. Повторите через {seconds} с."))
    }

    fn trip(&mut self, now: u64, duration_ms: u64, reason: &str) -> String {
        self.until_ms = now.saturating_add(duration_ms);
        self.reason = reason.to_string();
        self.error(now)
            .unwrap_or_else(|| format!("{reason}. Повторите позже."))
    }
}

pub fn search_key(query: &str) -> String {
    format!("{SEARCH_NAMESPACE}:{}", normalize_cache_part(query))
}

fn normalize_cache_part(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.trim().to_lowercase().chars() {
        if character.is_alphanumeric() || matches!(character, '-' | '_') {
            output.push(character);
            separator = false;
        } else if character.is_whitespace() {
            if !output.is_empty() && !output.ends_with(' ') {
                output.push(' ');
            }
            separator = false;
        } else if !separator && !output.is_empty() {
            output.push('_');
            separator = true;
        }
    }
    output.trim_end_matches([' ', '_']).to_string()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

/// Host imports are only callable inside the WASM component. Unit tests run on
/// the host, so cache/log are no-ops there; their logic is still exercised via
/// the pure functions (TTL, breaker, header policy).
#[cfg(target_arch = "wasm32")]
fn cache_read<T: DeserializeOwned>(key: &str) -> Option<T> {
    anm_sdk::cache_get(key).ok().flatten()
}

#[cfg(not(target_arch = "wasm32"))]
fn cache_read<T: DeserializeOwned>(_key: &str) -> Option<T> {
    None
}

#[cfg(target_arch = "wasm32")]
fn cache_write<T: Serialize>(key: &str, value: &T) -> Result<(), String> {
    anm_sdk::cache_set(key, value)
}

#[cfg(not(target_arch = "wasm32"))]
fn cache_write<T: Serialize>(_key: &str, _value: &T) -> Result<(), String> {
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn warn(message: &str) {
    anm_sdk::log_warn(message);
}

#[cfg(not(target_arch = "wasm32"))]
fn warn(message: &str) {
    eprintln!("{message}");
}

fn page_fresh(page: &CachedPage, now: u64, ttl_ms: u64) -> bool {
    now.saturating_sub(page.saved_at) < ttl_ms
}

fn stale_body(page: Option<&CachedPage>, now: u64) -> Option<String> {
    let page = page?;
    (now.saturating_sub(page.saved_at) <= STALE_MAX_MS).then(|| page.body.clone())
}

fn missing_key(cache_key: &str) -> String {
    format!("{cache_key}:missing")
}

// Forced reads must not return successful cached data, including stale fallbacks.
// Negative responses, pacing and circuit breaker state remain in effect.
fn cache_for_request(page: Option<CachedPage>, refresh: bool) -> Option<CachedPage> {
    page.filter(|_| !refresh)
}

fn load_page(key: &str) -> Option<CachedPage> {
    cache_read::<CachedPage>(key)
}

fn save_page(key: &str, page: &CachedPage) {
    if let Err(error) = cache_write(key, page) {
        warn(&format!(
            "MyAnimeList: не удалось сохранить кэш {key}: {error}"
        ));
    }
}

fn load_missing(key: &str) -> Option<MissingPage> {
    cache_read::<MissingPage>(&missing_key(key))
}

fn clear_missing(key: &str) {
    let _ = cache_write(
        &missing_key(key),
        &MissingPage {
            until_ms: 0,
            message: String::new(),
        },
    );
}

fn breaker_error(now: u64) -> Option<String> {
    let mut breaker = BREAKER.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(persisted) = cache_read::<Breaker>(BREAKER_KEY) {
        if persisted.until_ms > breaker.until_ms {
            *breaker = persisted;
        }
    }
    breaker.error(now)
}

fn trip_breaker(duration_ms: u64, reason: &str) -> String {
    let now = now_ms();
    let mut breaker = BREAKER.lock().unwrap_or_else(|error| error.into_inner());
    let message = breaker.trip(now, duration_ms, reason);
    let _ = cache_write(BREAKER_KEY, &*breaker);
    message
}

fn request_gate(
    cached: Option<&CachedPage>,
    breaker: Option<String>,
    now: u64,
    path: &str,
) -> Result<Option<String>, String> {
    match breaker {
        Some(error) => {
            if let Some(body) = stale_body(cached, now) {
                warn(&format!(
                    "MyAnimeList: {path}: {error} Используются данные из кэша."
                ));
                Ok(Some(body))
            } else {
                Err(error)
            }
        }
        // No cooldown: expired data must be revalidated even when a stale copy exists.
        None => Ok(None),
    }
}

fn wait_for_slot() {
    let mut last_request = LAST_REQUEST
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let persisted = cache_read::<u64>(PACING_KEY).unwrap_or(0);
    let previous = (*last_request).max(persisted);
    // A clock jump into the future must not stall the module forever.
    let delay = previous
        .saturating_add(MIN_INTERVAL_MS)
        .saturating_sub(now_ms())
        .min(MIN_INTERVAL_MS);
    if delay > 0 {
        std::thread::sleep(Duration::from_millis(delay));
    }
    *last_request = now_ms();
    let _ = cache_write(PACING_KEY, &*last_request);
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn retry_after_seconds(retry_after: Option<&str>) -> Option<u64> {
    retry_after?.trim().parse::<u64>().ok()
}

pub fn cooldown_for_status(status: u16, retry_after: Option<&str>) -> Option<u64> {
    match status {
        429 => Some(
            retry_after_seconds(retry_after)
                .map(|seconds| seconds.saturating_mul(1_000))
                .unwrap_or(FAILURE_COOLDOWN_MS)
                .clamp(FAILURE_COOLDOWN_MS, 10 * 60 * 1_000),
        ),
        403 => Some(CHALLENGE_COOLDOWN_MS),
        500..=599 => Some(
            retry_after_seconds(retry_after)
                .map(|seconds| seconds.saturating_mul(1_000))
                .unwrap_or(FAILURE_COOLDOWN_MS)
                .clamp(FAILURE_COOLDOWN_MS, 2 * 60 * 1_000),
        ),
        _ => None,
    }
}

fn retry_delay(status: u16, attempt: usize, retry_after: Option<&str>) -> Option<Duration> {
    if attempt >= 2 || !(500..=599).contains(&status) {
        return None;
    }
    let seconds = retry_after_seconds(retry_after).unwrap_or(1 << attempt);
    // A longer server cooldown cannot safely fit the host's per-call budget.
    (seconds <= 5).then(|| Duration::from_secs(seconds.max(1)))
}

fn network_failure_reason(error: &str) -> &'static str {
    if error.contains("connection timeout") {
        "MyAnimeList: не удалось установить соединение с myanimelist.net за отведённое время"
    } else if error.contains("request timeout") {
        "MyAnimeList: сервер myanimelist.net не ответил за отведённое время"
    } else if error.contains("domain not allowed") {
        "MyAnimeList: хост запретил доступ к myanimelist.net"
    } else {
        "Нет соединения с MyAnimeList (myanimelist.net)"
    }
}

fn status_error(status: u16, body: &[u8], path: &str) -> String {
    let snippet = html::excerpt(&String::from_utf8_lossy(body), 140);
    if snippet.is_empty() {
        format!("MyAnimeList: HTTP {status} для {path}")
    } else {
        format!("MyAnimeList: HTTP {status} для {path}: {snippet}")
    }
}

fn missing_error(path: &str) -> String {
    format!("MyAnimeList: страница не найдена ({path})")
}

fn request_headers(cached: Option<&CachedPage>) -> Vec<(String, String)> {
    let mut headers = vec![
        ("user-agent".to_string(), USER_AGENT.to_string()),
        ("accept".to_string(), ACCEPT.to_string()),
        ("accept-language".to_string(), ACCEPT_LANGUAGE.to_string()),
    ];
    if let Some(page) = cached {
        if let Some(etag) = &page.etag {
            headers.push(("if-none-match".to_string(), etag.clone()));
        }
        if let Some(last_modified) = &page.last_modified {
            headers.push(("if-modified-since".to_string(), last_modified.clone()));
        }
    }
    headers
}

/// Fetch a MAL page through the host allowlist with all anti-abuse safeguards.
/// `cache_key` is stable and independent from the URL slug; `path` must start
/// with `/`. Forced metadata reads report network failures instead of stale success.
pub fn get_page(cache_key: &str, path: &str, ttl_ms: u64) -> Result<String, String> {
    let now = now_ms();
    let mut cached = cache_for_request(load_page(cache_key), anm_sdk::force_refresh());
    if cached
        .as_ref()
        .is_some_and(|page| page_fresh(page, now, ttl_ms))
    {
        return Ok(cached.expect("checked above").body);
    }

    if let Some(missing) = load_missing(cache_key) {
        if missing.until_ms > now {
            return Err(missing.message);
        }
    }

    if let Some(body) = request_gate(cached.as_ref(), breaker_error(now), now, path)? {
        return Ok(body);
    }

    let url = html::absolute_path(path);
    let mut last_error = String::new();

    for attempt in 0..3 {
        wait_for_slot();
        let headers = request_headers(cached.as_ref());
        let response = match anm_sdk::http_request(&HttpRequest {
            url: url.clone(),
            method: None,
            headers,
            body: None,
        }) {
            Ok(response) => response,
            Err(error) => {
                warn(&format!(
                    "MyAnimeList: network request failed for {path}: {error}"
                ));
                last_error = trip_breaker(FAILURE_COOLDOWN_MS, network_failure_reason(&error));
                break;
            }
        };

        match response.status {
            200..=299 => {
                let body = String::from_utf8_lossy(&response.body).into_owned();
                if html::is_challenge_page(&body) {
                    last_error = trip_breaker(
                        CHALLENGE_COOLDOWN_MS,
                        "MyAnimeList вернул страницу проверки Cloudflare",
                    );
                    warn(&format!("MyAnimeList: challenge page for {path}"));
                    break;
                }
                if !html::looks_like_mal_page(&body) {
                    last_error = format!("MyAnimeList: неожиданный ответ для {path}");
                    warn(&last_error);
                    break;
                }
                let page = CachedPage::from_response(path, body, &response.headers, now_ms());
                save_page(cache_key, &page);
                clear_missing(cache_key);
                return Ok(page.body);
            }
            304 => {
                if let Some(mut page) = cached.take() {
                    page.saved_at = now_ms();
                    save_page(cache_key, &page);
                    return Ok(page.body);
                }
                last_error = format!("MyAnimeList: HTTP 304 без локальной копии ({path})");
                break;
            }
            404 => {
                let message = missing_error(path);
                let _ = cache_write(
                    &missing_key(cache_key),
                    &MissingPage {
                        until_ms: now_ms().saturating_add(MISSING_TTL_MS),
                        message: message.clone(),
                    },
                );
                return Err(message);
            }
            429 => {
                let retry_after = header_value(&response.headers, "retry-after");
                let duration = cooldown_for_status(429, retry_after).unwrap_or(FAILURE_COOLDOWN_MS);
                last_error = trip_breaker(duration, "MyAnimeList ограничил частоту запросов");
                warn(&format!(
                    "MyAnimeList: 429 for {path}; retry-after={retry_after:?}"
                ));
                break;
            }
            403 => {
                last_error = trip_breaker(
                    CHALLENGE_COOLDOWN_MS,
                    "MyAnimeList отклонил запрос (защита от автоматических обращений)",
                );
                warn(&format!("MyAnimeList: 403 for {path}"));
                break;
            }
            500..=599 => {
                if html::is_challenge_page(&String::from_utf8_lossy(&response.body)) {
                    last_error = trip_breaker(
                        CHALLENGE_COOLDOWN_MS,
                        "MyAnimeList вернул страницу проверки Cloudflare",
                    );
                    warn(&format!("MyAnimeList: challenge response for {path}"));
                    break;
                }
                last_error = status_error(response.status, &response.body, path);
                let retry_after = header_value(&response.headers, "retry-after");
                warn(&last_error);
                if let Some(delay) = retry_delay(response.status, attempt, retry_after) {
                    std::thread::sleep(delay);
                    continue;
                }
                if let Some(duration) = cooldown_for_status(response.status, retry_after) {
                    last_error = trip_breaker(duration, "Сервис MyAnimeList временно недоступен");
                }
                break;
            }
            status => {
                last_error = status_error(status, &response.body, path);
                warn(&last_error);
                break;
            }
        }
    }

    if let Some(body) = stale_body(cached.as_ref(), now_ms()) {
        warn(&format!(
            "MyAnimeList: {path}: {last_error}. Показаны устаревшие данные из кэша."
        ));
        return Ok(body);
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_retryable_statuses_open_bounded_cooldowns() {
        assert_eq!(cooldown_for_status(429, Some("60")), Some(60_000));
        assert_eq!(cooldown_for_status(429, Some("2")), Some(30_000));
        assert_eq!(cooldown_for_status(429, Some("3600")), Some(600_000));
        assert_eq!(cooldown_for_status(403, None), Some(600_000));
        assert_eq!(cooldown_for_status(503, None), Some(30_000));
        assert_eq!(cooldown_for_status(500, Some("30")), Some(30_000));
        assert_eq!(cooldown_for_status(404, Some("60")), None);
        assert_eq!(cooldown_for_status(200, None), None);
    }

    #[test]
    fn retries_are_bounded_and_respect_small_server_delays() {
        assert_eq!(retry_delay(503, 0, None), Some(Duration::from_secs(1)));
        assert_eq!(retry_delay(503, 1, None), Some(Duration::from_secs(2)));
        assert_eq!(retry_delay(503, 0, Some("4")), Some(Duration::from_secs(4)));
        assert_eq!(retry_delay(429, 0, Some("2")), None);
        assert_eq!(retry_delay(503, 0, Some("60")), None);
        assert_eq!(retry_delay(404, 0, None), None);
        assert_eq!(retry_delay(503, 2, None), None);
    }

    #[test]
    fn newly_cached_sections_keep_validators_with_only_the_parser_slice() {
        let path = "/anime/20/_/characters";
        let cast = r#"<div class="anime-character-container"><table>cast</table></div>"#;
        let body = format!("<html>{cast}<h2>Reviews</h2><p>large unrelated section</p></html>");
        let page = CachedPage::from_response(
            path,
            body,
            &[
                ("ETag".into(), "old-version".into()),
                ("Last-Modified".into(), "yesterday".into()),
            ],
            1_000,
        );
        assert_eq!(page.body, cast);
        assert_eq!(page.saved_at, 1_000);
        assert_eq!(html::trim_for(path, &page.body), cast);
        let headers = request_headers(Some(&page));
        assert_eq!(header_value(&headers, "if-none-match"), Some("old-version"));
        assert_eq!(header_value(&headers, "if-modified-since"), Some("yesterday"));
    }

    #[test]
    fn cache_is_fresh_only_inside_ttl() {
        let page = CachedPage {
            saved_at: 1_000,
            etag: None,
            last_modified: None,
            body: "<html></html>".into(),
        };
        assert!(page_fresh(&page, 1_000 + TTL_DETAIL_MS - 1, TTL_DETAIL_MS));
        assert!(!page_fresh(&page, 1_000 + TTL_DETAIL_MS, TTL_DETAIL_MS));
        assert_eq!(
            stale_body(Some(&page), 1_001).as_deref(),
            Some("<html></html>")
        );
    }

    #[test]
    fn breaker_fails_fast_and_keeps_cached_pages_usable() {
        let mut breaker = Breaker::default();
        assert!(breaker.error(1_000).is_none());
        let error = breaker.trip(1_000, FAILURE_COOLDOWN_MS, "Нет соединения");
        assert!(error.contains("30 с."));
        assert_eq!(breaker.error(1_001).as_deref(), Some(error.as_str()));
        assert!(breaker.error(31_000).is_none());
        let cached = CachedPage {
            saved_at: 1_000,
            etag: None,
            last_modified: None,
            body: "cached".into(),
        };
        assert_eq!(
            request_gate(Some(&cached), Some(error), 1_001, "/anime/20").unwrap(),
            Some("cached".into())
        );
        assert_eq!(
            request_gate(Some(&cached), None, 1_001, "/anime/20").unwrap(),
            None,
            "expired data must be revalidated when the breaker is closed"
        );
        assert!(request_gate(None, Some("fail".into()), 1_001, "/anime/20").is_err());
    }

    #[test]
    fn forced_refresh_bypasses_successful_pages_without_bypassing_breakers() {
        let page = CachedPage {
            saved_at: 1_000, etag: Some("old".into()),
            last_modified: Some("yesterday".into()), body: "cached".into(),
        };
        assert!(cache_for_request(Some(page.clone()), false).is_some());
        let refreshed = cache_for_request(Some(page), true);
        assert!(refreshed.is_none());
        assert!(request_headers(refreshed.as_ref()).iter().all(|(key, _)| !key.starts_with("if-")));
        assert!(stale_body(refreshed.as_ref(), 1_001).is_none());
        assert!(request_gate(refreshed.as_ref(), Some("cooldown".into()), 1_001, "/anime/20").is_err());
    }

    #[test]
    fn keys_are_stable_and_slug_independent() {
        let entry = crate::model::EntryRef::parse("manga:2").unwrap();
        assert_eq!(entry.page_key(None), "mal:v1:page:manga:2");
        assert_eq!(entry.page_key(Some("stats")), "mal:v1:page:manga:2:stats");
        assert_eq!(
            search_key("  Naruto Shippuden  "),
            "mal:v1:search:naruto shippuden"
        );
        assert_eq!(search_key("Bleach/??"), "mal:v1:search:bleach");
    }

    #[test]
    fn network_failures_are_distinct_from_api_errors() {
        assert!(
            network_failure_reason("upstream connection timeout").contains("установить соединение")
        );
        assert!(network_failure_reason("upstream request timeout").contains("не ответил"));
        assert!(
            network_failure_reason("domain not allowed by permissions").contains("запретил доступ")
        );
        assert!(status_error(500, b"<html><body>Oops</body></html>", "/anime/20").contains("Oops"));
    }
}
