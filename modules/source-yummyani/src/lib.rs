//! YummyAnime source module (`org.yummyani.source`).
//!
//! Implements the WIT `source-api` interface on top of the JSON API of
//! `old.yummyani.me`. Endpoints were verified against live API captures and
//! the site's own frontend client:
//!
//! - catalog:  `GET /api/anime?limit=…&offset=…&sort=…&status=…` (`sort`
//!   accepts `title`, `year`, `rating`, `rating_counters`, `views`, `top`,
//!   `random`, `id`; `status` accepts `ongoing`, `released`,
//!   `announcement`; every date-added or descending value is rejected by
//!   the site)
//! - search:   `GET /api/search?q=…&limit=…&offset=…` (the site client sends
//!   these parameters; whether the API honors `offset` past the first page
//!   is not verified, so no filter or sort is advertised for search)
//! - release:  `GET /api/anime/{anime_id}` (numeric id, `anime_url` is a slug)
//! - episodes: `GET /api/anime/{anime_id}/videos` (numbers, durations, skips)
//! - players:  a fresh `GET /api/anime/{anime_id}/videos` per request
//!
//! Every response is wrapped into `{"response": …}` or `{"error": …}`. Pages
//! hold 20 items and request one extra row to prove the next page; the extra
//! row only proves `has_next` and is never shown. The site has no updates
//! feed and rejects descending sorts, so the reserved `get_latest` and
//! `get_popular` feeds return a documented successful empty page instead of
//! inventing rows or mislabelling ascending views as popular. No fictional
//! filters are offered: only the validated `status` aliases above.
//!
//! Playback candidates are grouped per site player (`data.player_id`:
//! 1 Aksor, 2 Alloha, 3 CVH, 4 Kodik) with one dub variant per voice and
//! only for the canonical episode id of the requested release. The CVH
//! iframe must carry an unambiguous context naming the exact episode; the
//! Alloha iframe must carry every required parameter exactly once, never
//! blank, and its numeric `episode` must match the row. Embed URLs are
//! validated (HTTPS, exact host, no credentials, no non-default port)
//! before they reach the player modules; Alloha tokens stay inside the
//! ephemeral playback payload and are never inspected, cached or logged.
//!
//! All network access goes through `anm_sdk`, so the host enforces the
//! domain allowlist. Successful metadata caching belongs to the host;
//! release and episode metadata stays persistable (no opaque `sourceRef`).

wit_bindgen::generate!({
    path: "../../vendor/sdk/wit",
    world: "source-module-release-details",
});

use std::collections::BTreeMap;

use anm_sdk::protocol::provider_embed::{CvhContext, EmbedPayload, EmbedVariant};

use animex::module::types::{
    AiringStatus, AlternativeName, AnimePreview, CatalogPage, CatalogParams, Episode, EpisodeRef,
    FilterField, FilterSelect, InfoBlock, OptionPair, PageMeta, PlayerCandidate, Release,
    ReleaseRef, TimeSkip, TimeSkipType, TrackerData,
};
use exports::animex::module::catalog_sorting_api::Guest as SortingGuest;
use exports::animex::module::filter_ui_api::Guest as FilterUiGuest;
use exports::animex::module::source_api::Guest;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use url::Url;

const DEFAULT_MIRROR: &str = "https://old.yummyani.me";

/// Referer of the CVH integration page on the YummyAnime video host.
const CVH_REFERER: &str = "https://ru.yummyani.me/";
/// Referer of the API site for Kodik, Alloha and Aksor candidates.
const SITE_REFERER: &str = "https://old.yummyani.me/";

/// Host of the CVH integration iframe; the query carries the CVH media id.
const CVH_HOST: &str = "ru.yummyani.me";
const CVH_IFRAME_PATH: &str = "/iframeCVH.html";
/// Verified constants of the YummyAnime CVH integration (pub 745, aggr mali);
/// CVH keeps one season per media item.
const CVH_PUBLISHER: &str = "745";
const CVH_AGGREGATOR: &str = "mali";
const CVH_SEASON: u32 = 1;

/// The official Alloha iframe host and exact path.
const ALLOHA_HOST: &str = "alloha.yani.tv";
const ALLOHA_PATH: &str = "/";
/// Required Alloha parameters; values stay opaque and are never inspected.
const REQUIRED_ALLOHA_PARAMS: &[&str] =
    &["token_movie", "translation", "season", "episode", "token"];

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";

/// Page size advertised to the host; one extra row is requested to prove
/// the next page without a second request.
const PAGE_SIZE: u32 = 20;
const PROBE_SIZE: u32 = PAGE_SIZE + 1;

/// Site player routing keys from the `data.player_id` video field.
const PLAYER_AKSOR: u32 = 1;
const PLAYER_ALLOHA: u32 = 2;
const PLAYER_CVH: u32 = 3;
const PLAYER_KODIK: u32 = 4;

/// Kodik embed domains, mirroring the reusable `org.kodik.player` module.
const KODIK_HOSTS: &[&str] = &[
    "kodikplayer.com",
    "kodik.info",
    "kodik.biz",
    "kodik.cc",
    "aniqit.com",
    "kodikonline.com",
];

/// Real ascending `sort` values of the site API; anything else is rejected.
const SORTS: &[&str] = &["year", "title", "rating", "views", "id"];
/// Real `status` aliases accepted by the site catalog; anything else is
/// rejected without echoing the raw selection.
const STATUSES: &[&str] = &["ongoing", "released", "announcement"];

// ---------------------------------------------------------------------------
// Host HTTP helpers (no independent metadata cache).
// ---------------------------------------------------------------------------

fn base() -> String {
    let mirror = anm_sdk::config_get("mirror")
        .ok()
        .flatten()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    mirror.unwrap_or_else(|| DEFAULT_MIRROR.to_string())
}

/// The host owns successful metadata caching, expiry and forced refresh.
/// Playback video lists are always requested fresh by `get_players`.
fn fetch(url: &str) -> Result<String, String> {
    let response = anm_sdk::http_request(&anm_sdk::HttpRequest {
        url: url.to_string(),
        method: None,
        headers: vec![
            ("user-agent".to_string(), USER_AGENT.to_string()),
            ("referer".to_string(), SITE_REFERER.to_string()),
            ("x-requested-with".to_string(), "XMLHttpRequest".to_string()),
        ],
        body: None,
    })?;
    if !(200..300).contains(&response.status) {
        return Err(format!("http status {} for {url}", response.status));
    }
    Ok(String::from_utf8_lossy(&response.body).into_owned())
}

// ---------------------------------------------------------------------------
// Response envelope: `{"response": …}` or `{"error": …, "error_title": …}`.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    response: Option<T>,
    error: Option<String>,
    error_title: Option<String>,
}

fn decode<T: DeserializeOwned>(body: &str, context: &str) -> Result<T, String> {
    let envelope: Envelope<T> = serde_json::from_str(body)
        .map_err(|e| format!("yummyani {context}: invalid response ({e})"))?;
    match envelope.response {
        Some(response) => Ok(response),
        None => Err(format!(
            "yummyani {context}: {}",
            envelope
                .error
                .or(envelope.error_title)
                .unwrap_or_else(|| "empty response".to_string())
        )),
    }
}

// ---------------------------------------------------------------------------
// API models (unknown fields are ignored; absent fields default).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct AnimeItem {
    anime_id: Option<u64>,
    title: Option<String>,
    description: Option<String>,
    year: Option<u16>,
    poster: Option<Poster>,
    genres: Vec<Genre>,
    studios: Vec<Studio>,
    anime_status: Option<StatusInfo>,
    #[serde(rename = "type")]
    media_type: Option<TypeInfo>,
    rating: Option<RatingInfo>,
    remote_ids: Option<RemoteIds>,
    other_titles: Vec<String>,
    episodes: Option<EpisodesInfo>,
    min_age: Option<MinAge>,
    original: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Poster {
    fullsize: Option<String>,
    big: Option<String>,
    small: Option<String>,
    medium: Option<String>,
    huge: Option<String>,
    mega: Option<String>,
}

impl Poster {
    /// Medium webp first, then progressively rarer sizes, as an absolute URL.
    fn best(&self) -> Option<String> {
        [
            self.medium.as_deref(),
            self.big.as_deref(),
            self.small.as_deref(),
            self.fullsize.as_deref(),
            self.huge.as_deref(),
            self.mega.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(absolute)
        .find(|value| !value.is_empty())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Genre {
    id: Option<u64>,
    title: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Studio {
    title: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct StatusInfo {
    value: Option<u32>,
    title: Option<String>,
    alias: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct TypeInfo {
    name: Option<String>,
    shortname: Option<String>,
    alias: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RatingInfo {
    average: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RemoteIds {
    shikimori_id: Option<u64>,
    myanimelist_id: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct EpisodesInfo {
    count: Option<u64>,
    next_date: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct MinAge {
    title_long: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct VideoRow {
    data: Option<VideoData>,
    number: Option<serde_json::Value>,
    iframe_url: Option<String>,
    duration: Option<u64>,
    skips: Option<Skips>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct VideoData {
    player_id: Option<u32>,
    dubbing: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Skips {
    opening: Option<Span>,
    ending: Option<Span>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Span {
    time: Option<u32>,
    length: Option<u32>,
}

// ---------------------------------------------------------------------------
// Small shared helpers.
// ---------------------------------------------------------------------------

fn absolute(value: &str) -> String {
    let value = value.trim();
    match value.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => value.to_string(),
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// UI selections are JSON objects; older callers may send JSON strings or
/// plain values. Decode the wire format before mapping it to site parameters.
fn option_value(value: &str) -> String {
    let value = value.trim();
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(serde_json::Value::String(value)) => value,
        Ok(serde_json::Value::Object(value)) => value
            .get("value")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => value.to_string(),
    }
}

/// The episode number of a video row: the site usually sends a string.
fn row_episode_number(row: &VideoRow) -> Option<u32> {
    let value = row.number.as_ref()?;
    match value {
        serde_json::Value::String(text) => text.trim().parse::<u32>().ok(),
        serde_json::Value::Number(number) => {
            number.as_u64().and_then(|value| u32::try_from(value).ok())
        }
        _ => None,
    }
}

fn numeric_release_id(id: &str) -> Result<String, String> {
    let value = id.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("yummyani: release id must be the numeric site anime_id".into());
    }
    Ok(value.to_string())
}

/// The canonical episode id of this release, exactly as `episodes_from_rows`
/// formats it; ids from other releases or bare numbers are rejected.
fn episode_number_of(release_id: &str, episode_id: &str) -> Result<u32, String> {
    let prefix = format!("{release_id}-");
    episode_id
        .strip_prefix(&prefix)
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|number| *number > 0)
        .ok_or_else(|| "yummyani: episode does not belong to this release".to_string())
}

/// Unix seconds → RFC3339 UTC, without an external time crate
/// (Howard Hinnant's `civil_from_days`).
fn rfc3339_utc(seconds: u64) -> Option<String> {
    let days = (seconds / 86_400) as i64;
    let time = seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    if !(1..=9999).contains(&year) {
        return None;
    }
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    ))
}

// ---------------------------------------------------------------------------
// Field mapping.
// ---------------------------------------------------------------------------

fn status_of(item: &AnimeItem) -> AiringStatus {
    let Some(status) = item.anime_status.as_ref() else {
        return AiringStatus::Unknown;
    };
    if let Some(alias) = status.alias.as_deref() {
        match alias {
            "ongoing" => return AiringStatus::Ongoing,
            "released" => return AiringStatus::Released,
            "announcement" => return AiringStatus::Anons,
            _ => {}
        }
    }
    match status.value {
        Some(1) => AiringStatus::Ongoing,
        Some(0) => AiringStatus::Released,
        Some(2) => AiringStatus::Anons,
        _ => AiringStatus::Unknown,
    }
}

fn kind_of(item: &AnimeItem) -> Option<String> {
    let info = item.media_type.as_ref()?;
    if let Some(alias) = clean(info.alias.clone()) {
        return Some(alias);
    }
    let label = info
        .name
        .as_deref()
        .or(info.shortname.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    kind_from_label(label).map(str::to_string)
}

fn kind_from_label(label: &str) -> Option<&'static str> {
    match label.to_lowercase().as_str() {
        label if label.contains("сериал") || label.contains("тв") => Some("tv"),
        label if label.contains("фильм") => Some("movie"),
        label if label.contains("ова") => Some("ova"),
        label if label.contains("она") => Some("ona"),
        label if label.contains("спешл") => Some("special"),
        _ => None,
    }
}

/// Real tracker identifiers only, never zero values.
fn tracker_data_of(item: &AnimeItem) -> Vec<TrackerData> {
    let Some(remote) = item.remote_ids.as_ref() else {
        return Vec::new();
    };
    [
        ("Shikimori", remote.shikimori_id),
        ("MyAnimeList", remote.myanimelist_id),
    ]
    .into_iter()
    .filter_map(|(name, id)| {
        let id = id.filter(|value| *value > 0)?;
        Some(TrackerData {
            name: name.to_string(),
            data: id.to_string(),
        })
    })
    .collect()
}

/// All available names with the same language heuristic as AnimeGO.
fn alternative_names(item: &AnimeItem, title: &str) -> Vec<AlternativeName> {
    let mut result: Vec<AlternativeName> = Vec::new();
    for name in item
        .other_titles
        .iter()
        .filter_map(|name| clean(Some(name.clone())))
    {
        if name.eq_ignore_ascii_case(title)
            || result
                .iter()
                .any(|known| known.name.eq_ignore_ascii_case(&name))
        {
            continue;
        }
        let language = if name.chars().any(|c| {
            (0x3040..=0x30FF).contains(&(c as u32)) || (0x4E00..=0x9FFF).contains(&(c as u32))
        }) {
            "ja"
        } else if name
            .chars()
            .any(|c| (0x0400..=0x04FF).contains(&(c as u32)))
        {
            "ru"
        } else {
            "unknown"
        };
        result.push(AlternativeName {
            name,
            language: language.to_string(),
        });
    }
    result
}

fn other_info_of(item: &AnimeItem) -> Vec<InfoBlock> {
    let mut blocks = Vec::new();
    for (title, value) in [
        (
            "Тип",
            item.media_type
                .as_ref()
                .and_then(|info| clean(info.name.clone())),
        ),
        (
            "Статус",
            item.anime_status
                .as_ref()
                .and_then(|status| clean(status.title.clone())),
        ),
        (
            "Возраст",
            item.min_age
                .as_ref()
                .and_then(|age| clean(age.title_long.clone())),
        ),
        ("Первоисточник", clean(item.original.clone())),
    ] {
        if let Some(value) = value {
            blocks.push(InfoBlock {
                title: title.to_string(),
                data: vec![value],
            });
        }
    }
    blocks
}

fn preview_from_item(item: &AnimeItem) -> AnimePreview {
    let title = clean(item.title.clone()).unwrap_or_default();
    AnimePreview {
        id: item.anime_id.map(|id| id.to_string()).unwrap_or_default(),
        title,
        alternative_names: Vec::new(),
        genres: item
            .genres
            .iter()
            .filter_map(|genre| clean(genre.title.clone()))
            .collect(),
        description: clean(item.description.clone()),
        poster: item.poster.as_ref().and_then(|poster| poster.best()),
        year: item.year,
        score: item
            .rating
            .as_ref()
            .and_then(|rating| rating.average)
            .filter(|average| *average > 0.0)
            .map(|average| average as f32),
        kind: kind_of(item),
        status: status_of(item),
        episodes_count: None,
        tracker_data: tracker_data_of(item),
        source_ref: None,
    }
}

fn map_release(item: &AnimeItem, requested_id: &str) -> Result<Release, String> {
    let title = clean(item.title.clone())
        .ok_or_else(|| format!("yummyani: release {requested_id} has no title"))?;
    let poster = item.poster.as_ref().and_then(|poster| poster.best());
    // No opaque sourceRef: the numeric id is enough for episodes and players,
    // and release metadata stays persistable in the host cache.
    Ok(Release {
        studio_details: None, episode_duration_seconds: None, aired_from: None, aired_to: None, regional_premieres: vec![], age_rating: None, original_source: None, score_scale: None, url: None,

        id: requested_id.to_string(),
        alternative_names: alternative_names(item, &title),
        tracker_data: tracker_data_of(item),
        other_info: other_info_of(item),
        banner_url: poster.clone(),
        poster,
        description: clean(item.description.clone()),
        genres: release_metadata::genres(item.genres.iter().filter_map(|genre| clean(genre.title.clone())).collect()),
        studios: item
            .studios
            .iter()
            .filter_map(|studio| clean(studio.title.clone()))
            .collect(),
        status: status_of(item),
        kind: kind_of(item),
        year: item.year,
        trailer: None,
        total_episodes: item
            .episodes
            .as_ref()
            .and_then(|episodes| episodes.count)
            .and_then(|count| u32::try_from(count).ok()),
        next_episode_at: item
            .episodes
            .as_ref()
            .and_then(|episodes| episodes.next_date)
            .and_then(rfc3339_utc),
        rating: item
            .rating
            .as_ref()
            .and_then(|rating| rating.average)
            .filter(|average| *average > 0.0)
            .map(|average| average as f32),
        name: title,
        source_ref: None,
    })
}

// ---------------------------------------------------------------------------
// Episodes and time skips from the videos list.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct EpisodeMetadata<'a> {
    duration: Option<u64>,
    opening: Option<&'a Span>,
    ending: Option<&'a Span>,
}

fn episodes_from_rows(rows: &[VideoRow], release_id: &str) -> Vec<Episode> {
    let mut episodes = BTreeMap::<u32, EpisodeMetadata<'_>>::new();
    for row in rows {
        let Some(number) = row_episode_number(row) else {
            continue;
        };
        let metadata = episodes.entry(number).or_default();
        metadata.duration = metadata.duration.max(row.duration.filter(|value| *value > 0));
        if let Some(skips) = &row.skips {
            // Preserve the first reported span for each kind, even if it is invalid.
            metadata.opening = metadata.opening.or(skips.opening.as_ref());
            metadata.ending = metadata.ending.or(skips.ending.as_ref());
        }
    }

    episodes
        .into_iter()
        .map(|(number, metadata)| Episode {
            id: format!("{release_id}-{number}"),
            number,
            title: None,
            thumbnail: None,
            duration: metadata.duration.map(|value| value as f32),
            time_skips: [
                (TimeSkipType::Opening, metadata.opening),
                (TimeSkipType::Ending, metadata.ending),
            ]
            .into_iter()
            .filter_map(|(kind, span)| time_skip(kind, span?))
            .collect(),
            source_ref: None,
        })
        .collect()
}

fn time_skip(kind: TimeSkipType, span: &Span) -> Option<TimeSkip> {
    let start = span.time?;
    let length = span.length.filter(|length| *length > 0)?;
    let end = start.checked_add(length)?;
    Some(TimeSkip {
        kind,
        start: start as f32,
        end: end as f32,
    })
}

// ---------------------------------------------------------------------------
// Player routing: exact host/path checks, one candidate per site player.
// ---------------------------------------------------------------------------

/// Embed URLs are accepted only as absolute HTTPS without credentials or a
/// non-default port; the site itself only emits `//host/…` forms.
fn embed_url(row: &VideoRow) -> Option<Url> {
    let raw = row.iframe_url.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }
    let url = Url::parse(&absolute(raw)).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    Some(url)
}

/// CVH integration context from the iframe query: `anime_id` is the CVH media
/// id, `dubbing_code` the CVH voice and `episode` the exact episode. The
/// publisher/aggregator/season constants are verified integration settings.
/// Repeated or blank context parameters make the row ambiguous: it is
/// skipped, never guessed.
fn cvh_context(url: &Url) -> Option<CvhContext> {
    if url.host_str() != Some(CVH_HOST) || url.path() != CVH_IFRAME_PATH {
        return None;
    }
    let mut media_id = None;
    let mut voice = None;
    let mut episode = None;
    for (key, value) in url.query_pairs() {
        let value = value.into_owned();
        let slot = match key.as_ref() {
            "anime_id" => &mut media_id,
            "dubbing_code" => &mut voice,
            "episode" => &mut episode,
            _ => continue,
        };
        if slot.replace(value).is_some() {
            return None;
        }
    }
    Some(CvhContext {
        media_id: clean(media_id)?,
        voice: clean(voice)?,
        publisher_id: CVH_PUBLISHER.to_string(),
        aggregator: CVH_AGGREGATOR.to_string(),
        season: CVH_SEASON,
        episode: episode?.parse().ok()?,
    })
}

/// The official Alloha iframe: exact host, exact `/` path, every required
/// parameter present exactly once and never blank. A clearly numeric
/// `episode` parameter must name the row's episode. Parameter values stay
/// opaque and are never inspected or logged.
fn alloha_url(url: &Url, number: Option<u32>) -> bool {
    if url.host_str() != Some(ALLOHA_HOST) || url.path() != ALLOHA_PATH {
        return false;
    }
    let mut present: Vec<String> = Vec::new();
    let mut episode = None;
    for (name, value) in url.query_pairs() {
        let name = name.into_owned();
        if !REQUIRED_ALLOHA_PARAMS.contains(&name.as_str()) {
            continue;
        }
        if present.contains(&name) || value.trim().is_empty() {
            return false;
        }
        if name == "episode" {
            episode = Some(value.into_owned());
        }
        present.push(name);
    }
    if present.len() != REQUIRED_ALLOHA_PARAMS.len() {
        return false;
    }
    match (
        episode
            .as_deref()
            .and_then(|value| value.trim().parse::<u32>().ok()),
        number,
    ) {
        (Some(episode), Some(number)) => episode == number,
        _ => true,
    }
}

fn aksor_url(url: &Url) -> bool {
    if url.host_str() != Some("player.aksor.tv") {
        return false;
    }
    let segments: Vec<&str> = url
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    matches!(segments.as_slice(), ["video", id]
        if id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
}

fn kodik_url(url: &Url) -> bool {
    KODIK_HOSTS.contains(&url.host_str().unwrap_or_default())
}

fn player_candidates(rows: &[&VideoRow]) -> Vec<PlayerCandidate> {
    const ROUTES: &[(u32, &str, &str, &str)] = &[
        (PLAYER_CVH, "cvh.embed.v1", "CVH", CVH_REFERER),
        (PLAYER_KODIK, "kodik.embed.v1", "Kodik", SITE_REFERER),
        (PLAYER_ALLOHA, "alloha.embed.v1", "Alloha", SITE_REFERER),
        (PLAYER_AKSOR, "aksor.embed.v1", "Aksor", SITE_REFERER),
    ];
    let mut candidates = Vec::new();
    for (priority, (player_id, protocol, label, referer)) in ROUTES.iter().enumerate() {
        let mut variants: Vec<EmbedVariant> = Vec::new();
        for row in rows {
            let Some(data) = row.data.as_ref() else {
                continue;
            };
            if data.player_id != Some(*player_id) {
                continue;
            }
            let Some(dubbing) = clean(data.dubbing.clone()) else {
                continue;
            };
            let Some(url) = embed_url(row) else {
                continue;
            };
            let number = row_episode_number(row);
            let cvh = match *player_id {
                // The CVH iframe must carry an unambiguous, non-empty context
                // naming the exact episode; anything else is skipped.
                PLAYER_CVH => {
                    let context =
                        cvh_context(&url).filter(|context| Some(context.episode) == number);
                    if context.is_none() {
                        continue;
                    }
                    context
                }
                _ => None,
            };
            match *player_id {
                PLAYER_ALLOHA if !alloha_url(&url, number) => continue,
                PLAYER_AKSOR if !aksor_url(&url) => continue,
                PLAYER_KODIK if !kodik_url(&url) => continue,
                _ => {}
            }
            // The dub label is the exact, unique voice identity: no fallbacks.
            if variants
                .iter()
                .any(|variant| variant.id == dubbing || variant.label == dubbing)
            {
                continue;
            }
            variants.push(EmbedVariant {
                id: dubbing.clone(),
                label: dubbing,
                url: url.to_string(),
                language: Some("ru".to_string()),
                cvh,
            });
        }
        if variants.is_empty() {
            continue;
        }
        let payload = serde_json::to_string(&EmbedPayload {
            variants,
            referer: Some(referer.to_string()),
        })
        .map_err(|e| format!("yummyani: candidate payload ({e})"));
        if let Ok(payload) = payload {
            candidates.push(PlayerCandidate {
                protocol: protocol.to_string(),
                label: Some(label.to_string()),
                priority: Some(priority as u32),
                payload,
            });
        }
    }
    candidates
}

// ---------------------------------------------------------------------------
// Request building.
// ---------------------------------------------------------------------------

fn page_offset(page: u32) -> u32 {
    page.saturating_sub(1).saturating_mul(PAGE_SIZE)
}

fn api_url_for(base: &str, path: &str, query: &[(&str, String)]) -> Result<String, String> {
    let mut url = Url::parse(&format!("{base}{path}")).map_err(|e| format!("yummyani url: {e}"))?;
    url.query_pairs_mut()
        .extend_pairs(query.iter().map(|(key, value)| (*key, value.as_str())));
    Ok(url.to_string())
}

fn catalog_url_for(
    base: &str,
    page: u32,
    sort: Option<&str>,
    status: Option<&str>,
) -> Result<String, String> {
    let mut query = vec![
        ("limit", PROBE_SIZE.to_string()),
        ("offset", page_offset(page).to_string()),
    ];
    if let Some(sort) = sort {
        query.push(("sort", sort.to_string()));
    }
    if let Some(status) = status {
        query.push(("status", status.to_string()));
    }
    api_url_for(base, "/api/anime", &query)
}

fn search_url_for(base: &str, query: &str, page: u32) -> Result<String, String> {
    api_url_for(
        base,
        "/api/search",
        &[
            ("q", query.to_string()),
            ("limit", PROBE_SIZE.to_string()),
            ("offset", page_offset(page).to_string()),
        ],
    )
}

fn release_url_for(base: &str, id: &str) -> Result<String, String> {
    api_url_for(base, &format!("/api/anime/{id}"), &[])
}

fn videos_url_for(base: &str, id: &str) -> Result<String, String> {
    api_url_for(base, &format!("/api/anime/{id}/videos"), &[])
}

/// Only real ascending site sorts; unknown values fail without echoing the
/// raw selection.
fn sort_of(params: &CatalogParams) -> Result<Option<String>, String> {
    let value = params
        .sort
        .as_deref()
        .or_else(|| {
            params
                .filters
                .iter()
                .find(|(key, _)| key == "sort")
                .map(|(_, value)| value.as_str())
        })
        .map(option_value)
        .filter(|value| !value.is_empty());
    match value {
        None => Ok(None),
        Some(value) if SORTS.contains(&value.as_str()) => Ok(Some(value)),
        Some(_) => Err("yummyani: unsupported sort value".to_string()),
    }
}

/// Only the validated site status aliases; unknown values fail without
/// echoing the raw selection.
fn status_filter_of(params: &CatalogParams) -> Result<Option<String>, String> {
    let value = params
        .filters
        .iter()
        .find(|(key, _)| key == "status")
        .map(|(_, value)| value.as_str())
        .map(option_value)
        .filter(|value| !value.is_empty());
    match value {
        None => Ok(None),
        Some(value) if STATUSES.contains(&value.as_str()) => Ok(Some(value)),
        Some(_) => Err("yummyani: unsupported status filter".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Listing pages.
// ---------------------------------------------------------------------------

fn listing_page(body: &str, page: u32) -> Result<CatalogPage, String> {
    let raw: Vec<AnimeItem> = decode(body, "catalog")?;
    // The extra probe row only proves the next page; it is never shown.
    let has_next = raw.len() > PAGE_SIZE as usize;
    let mut items = raw;
    items.truncate(PAGE_SIZE as usize);
    let mut previews: Vec<AnimePreview> = items.iter().map(preview_from_item).collect();
    previews.retain(|preview| !preview.id.is_empty() && !preview.title.is_empty());
    let per_page = previews.len().max(1) as u32;
    Ok(CatalogPage {
        items: previews,
        meta: PageMeta {
            page,
            per_page,
            total: None,
            total_pages: None,
            has_next,
        },
    })
}

fn fetch_listing(url: &str, page: u32) -> Result<CatalogPage, String> {
    let body = fetch(url)?;
    listing_page(&body, page)
}

/// Successful empty page for reserved feeds the site cannot honestly
/// provide. Documented as "unsupported operation": never fictitious rows,
/// never a global home error.
fn unsupported_page(page: u32) -> CatalogPage {
    CatalogPage {
        items: Vec::new(),
        meta: PageMeta {
            page,
            per_page: 1,
            total: None,
            total_pages: None,
            has_next: false,
        },
    }
}

// ---------------------------------------------------------------------------
// Guest implementation.
// ---------------------------------------------------------------------------

struct Component;

impl FilterUiGuest for Component {
    fn get_filter_ui(
    ) -> Result<Vec<exports::animex::module::filter_ui_api::FilterPresentation>, String> {
        use exports::animex::module::filter_ui_api::{ChoicePresentation, FilterPresentation};
        // One validated filter: the site status aliases.
        Ok(vec![FilterPresentation {
            id: "status".to_string(),
            presentation: ChoicePresentation::Dropdown,
        }])
    }
}

impl SortingGuest for Component {
    fn get_sorting(
    ) -> Result<Option<exports::animex::module::catalog_sorting_api::SortingConfig>, String> {
        use exports::animex::module::catalog_sorting_api::SortingConfig;
        Ok(Some(SortingConfig {
            id: "sort".to_string(),
            label: "Сортировка".to_string(),
            options: sort_options(),
            default: None,
            // The site rejects descending sorts (`-year` is not in the enum).
            directions: Vec::new(),
            default_direction: None,
        }))
    }
}

fn sort_options() -> Vec<OptionPair> {
    labeled_options(&[
        ("year", "По году"),
        ("title", "По названию"),
        ("rating", "По рейтингу"),
        ("views", "По просмотрам"),
        ("id", "По идентификатору"),
    ])
}

fn status_options() -> Vec<OptionPair> {
    labeled_options(&[
        ("ongoing", "Онгоинг"),
        ("released", "Вышел"),
        ("announcement", "Анонс"),
    ])
}

fn labeled_options(values: &[(&str, &str)]) -> Vec<OptionPair> {
    values
        .iter()
        .map(|(value, label)| OptionPair {
            value: value.to_string(),
            label: label.to_string(),
        })
        .collect()
}

impl Guest for Component {
    fn get_filters() -> Result<Vec<FilterField>, String> {
        // Only the validated status aliases; no fictional filters.
        Ok(vec![FilterField::Select(FilterSelect {
            id: "status".to_string(),
            label: "Статус".to_string(),
            options: status_options(),
            default: None,
        })])
    }

    fn get_catalog(params: CatalogParams) -> Result<CatalogPage, String> {
        if let Some(query) = clean(params.search.clone()) {
            // The site client sends `q`, `limit` and `offset` to the search
            // endpoint; whether the API honors `offset` past the first page
            // is not verified, so no filter or sort is advertised for search.
            return Self::search(query, params);
        }
        let sort = sort_of(&params)?;
        let status = status_filter_of(&params)?;
        fetch_listing(
            &catalog_url_for(&base(), params.page, sort.as_deref(), status.as_deref())?,
            params.page,
        )
    }

    fn get_latest(page: u32) -> Result<CatalogPage, String> {
        // Reserved feed: the API `sort` enum rejects every date-added value,
        // so the site has no updates feed. A successful empty page represents
        // the unsupported operation without inventing rows and without
        // turning the home view's allSettled into a global error.
        Ok(unsupported_page(page))
    }

    fn get_popular(page: u32) -> Result<CatalogPage, String> {
        // Reserved feed: the API rejects descending sorts (`-views` is not
        // in the enum), so ascending views would label the least-viewed
        // titles as popular. No verified ordering exists; an empty page
        // avoids the mislabelling.
        Ok(unsupported_page(page))
    }

    fn search(query: String, params: CatalogParams) -> Result<CatalogPage, String> {
        fetch_listing(
            &search_url_for(&base(), query.trim(), params.page)?,
            params.page,
        )
    }

    fn get_release(release: ReleaseRef) -> Result<Release, String> {
        let id = numeric_release_id(&release.id)?;
        let body = fetch(&release_url_for(&base(), &id)?)?;
        let item: AnimeItem = decode(&body, "release")?;
        map_release(&item, &id)
    }

    fn get_episodes(release: ReleaseRef) -> Result<Vec<Episode>, String> {
        let id = numeric_release_id(&release.id)?;
        let body = fetch(&videos_url_for(&base(), &id)?)?;
        let rows: Vec<VideoRow> = decode(&body, "videos")?;
        Ok(episodes_from_rows(&rows, &id))
    }

    fn get_players(
        release: ReleaseRef,
        episode: EpisodeRef,
    ) -> Result<Vec<PlayerCandidate>, String> {
        let id = numeric_release_id(&release.id)?;
        let number = episode_number_of(&id, &episode.id)?;
        // A fresh videos list per request: embed tokens are ephemeral and
        // playback payloads are never cached or logged.
        let body = fetch(&videos_url_for(&base(), &id)?)?;
        let rows: Vec<VideoRow> = decode(&body, "videos")?;
        let scoped: Vec<&VideoRow> = rows
            .iter()
            .filter(|row| row_episode_number(row) == Some(number))
            .collect();
        if scoped.is_empty() {
            // No fallback to another episode or voice.
            return Err(format!("yummyani: episode {number} has no videos"));
        }
        Ok(player_candidates(&scoped))
    }
}

anm_sdk::release_metadata!();

impl exports::animex::module::release_details_api::Guest for Component {
    fn get_release_details(reference: ReleaseRef) -> Result<crate::animex::module::types::Release, String> {

        let id = numeric_release_id(&reference.id)?;
        let body = fetch(&release_url_for(&base(), &id)?)?;
        let item: AnimeItem = decode(&body, "release")?;
        let mut details = map_release(&item, &id)?;
        details.genres = item.genres.iter().filter_map(|genre| Some(crate::animex::module::types::Genre {
            name: clean(genre.title.clone())?, id: genre.id.map(|id| id.to_string()),
            language: Some("ru".into()), url: None,
        })).collect();
        details.age_rating = item.min_age.as_ref().and_then(|age| age.title_long.as_deref())
            .and_then(release_metadata::age_rating);
        details.score_scale = details.rating.map(|_| 10);
        details.studio_details = release_metadata::studios(&details.studios);
        Ok(details)
    }
}

export!(Component);

#[cfg(test)]
mod tests;
