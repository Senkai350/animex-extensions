//! First-party tracker module: Shikimori GraphQL (`org.shikimori.tracker`).
//! Character roles are read directly from GraphQL; REST is used for similar titles.

wit_bindgen::generate!({
    path: "../../vendor/sdk/wit",
    world: "tracker-module-release-details-character-roles",
});

mod anime_details;

use std::time::{SystemTime, UNIX_EPOCH};

use animex::module::types::{
    AiringStatus, AlternativeName, AnimePreview, Character, MediaType, RelatedTitle, Release,
    ReleaseRef, ScoreCount, Screenshot, SimilarTitle, StaffMember, StatusCount,
    TitleCard, TitleRelation, TitleStatistics, TrackerData, TrackerVideo, VideoKind,
};
use exports::animex::module::tracker_api::Guest;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const GRAPHQL: &str = "https://shikimori.io/api/graphql";
const SITE: &str = "https://shikimori.io";
const CACHE_TTL_SECS: u64 = 3600;
const RATE_LIMIT_ATTEMPTS: u8 = 3;

const SEARCH_QUERY: &str = r#"
query Search($q: String!, $limit: PositiveInt!) {
  animes(search: $q, limit: $limit) {
    id
    name
    russian
    english
    japanese
    synonyms
    description
    genres { name russian }
    kind
    status
    score
    episodes
    malId
    airedOn { year }
    poster { originalUrl previewUrl }
  }
}
"#;

const ANIME_QUERY: &str = r#"
query Anime($id: String!) {
  animes(ids: $id) {
    id
    name
    russian
    english
    japanese
    synonyms
    description
    kind
    status
    score
    episodes
    nextEpisodeAt
    opengraphImageUrl
    malId
    airedOn { year }
    poster { originalUrl previewUrl }
    genres { name russian }
    studios { id name imageUrl }
    videos { id kind name url playerUrl imageUrl }
    screenshots { originalUrl x166Url x332Url }
    related {
      relationKind
      relationText
      anime { id name russian kind status airedOn { year } poster { originalUrl } }
      manga { id name russian kind status airedOn { year } poster { originalUrl } }
    }
    characterRoles {
      rolesRu
      rolesEn
      character { id malId name russian poster { originalUrl } }
    }
    personRoles {
      rolesRu
      person { id name russian poster { originalUrl } }
    }
  }
}
"#;

const STATISTICS_QUERY: &str = r#"
query Statistics($id: String!) {
  animes(ids: $id, limit: 1) {
    id
    score
    scoresStats { score count }
    statusesStats { status count }
  }
}
"#;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StatisticsNode {
    id: String,
    score: Option<f32>,
    scores_stats: Option<Vec<RawScoreCount>>,
    statuses_stats: Option<Vec<RawStatusCount>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawScoreCount {
    score: i64,
    count: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawStatusCount {
    status: String,
    count: i64,
}
#[derive(Debug, Deserialize)]
struct StatisticsData {
    animes: Vec<StatisticsNode>,
}

fn map_statistics(raw: StatisticsNode) -> TitleStatistics {
    let mut scores = std::collections::BTreeMap::<u8, u32>::new();
    for row in raw.scores_stats.unwrap_or_default() {
        if !(1..=10).contains(&row.score) {
            continue;
        }
        if let Ok(count) = u32::try_from(row.count) {
            let value = scores.entry(row.score as u8).or_default();
            *value = value.saturating_add(count);
        }
    }
    let mut statuses = std::collections::BTreeMap::<String, u32>::new();
    for row in raw.statuses_stats.unwrap_or_default() {
        if row.status.trim().is_empty() {
            continue;
        }
        if let Ok(count) = u32::try_from(row.count) {
            let value = statuses.entry(row.status).or_default();
            *value = value.saturating_add(count);
        }
    }
    TitleStatistics {
        rating: raw
            .score
            .filter(|v| v.is_finite() && *v > 0.0 && *v <= 10.0),
        score_scale: 10,
        scores: scores
            .into_iter()
            .map(|(score, count)| ScoreCount { score, count })
            .collect(),
        statuses: statuses
            .into_iter()
            .map(|(status, count)| {
                let label = match status.as_str() {
                    "watching" => "Смотрю",
                    "planned" => "Запланировано",
                    "completed" => "Просмотрено",
                    "rewatching" => "Пересматриваю",
                    "on_hold" => "Отложено",
                    "dropped" => "Брошено",
                    _ => status.as_str(),
                }
                .to_string();
                StatusCount {
                    status,
                    label,
                    count,
                }
            })
            .collect(),
    }
}

impl exports::animex::module::statistics_api::Guest for Component {
    fn get_statistics(release: ReleaseRef) -> Result<TitleStatistics, String> {
        if release.id.is_empty() || !release.id.bytes().all(|c| c.is_ascii_digit()) {
            return Err("shikimori: invalid anime id".into());
        }
        let key = format!("shikimori:v1:statistics:{}", release.id);
        if let Ok(Some(cached)) = anm_sdk::data_cache_get::<CacheEntry<StatisticsNode>>(&key) {
            if now_secs().saturating_sub(cached.saved_at) < CACHE_TTL_SECS {
                return Ok(map_statistics(cached.data));
            }
        }
        let data: StatisticsData = gql(STATISTICS_QUERY, serde_json::json!({ "id": release.id }))?;
        let node = data
            .animes
            .into_iter()
            .find(|n| n.id == release.id)
            .ok_or_else(|| format!("shikimori: anime '{}' not found", release.id))?;
        let _ = anm_sdk::cache_set(
            &key,
            &CacheEntry {
                saved_at: now_secs(),
                data: node.clone(),
            },
        );
        Ok(map_statistics(node))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum IdValue {
    Num(i64),
    Text(String),
}

impl IdValue {
    fn as_string(&self) -> String {
        match self {
            IdValue::Num(value) => value.to_string(),
            IdValue::Text(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct IncompleteDate {
    year: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Poster {
    original_url: Option<String>,
    preview_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Genre {
    name: String,
    russian: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
struct Studio {
    name: String,
    id: Option<String>,
    image_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Video {
    id: String,
    kind: String,
    name: Option<String>,
    url: String,
    player_url: String,
    image_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct ScreenshotNode {
    #[serde(rename = "originalUrl")]
    original_url: String,
    #[serde(rename = "x166Url")]
    x166_url: String,
    #[serde(rename = "x332Url")]
    x332_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RelatedNode {
    relation_kind: String,
    relation_text: String,
    anime: Option<RelatedAnimeNode>,
    manga: Option<RelatedAnimeNode>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RelatedAnimeNode {
    id: String,
    name: String,
    russian: Option<String>,
    poster: Option<Poster>,
    kind: Option<String>,
    status: Option<String>,
    aired_on: Option<IncompleteDate>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CharacterNode {
    id: String,
    mal_id: Option<IdValue>,
    name: String,
    russian: Option<String>,
    poster: Option<Poster>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CharacterRole {
    roles_ru: Vec<String>,
    roles_en: Vec<String>,
    character: CharacterNode,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PersonNode {
    id: String,
    name: String,
    russian: Option<String>,
    poster: Option<Poster>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PersonRole {
    roles_ru: Vec<String>,
    person: PersonNode,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct AnimeNode {
    id: String,
    name: String,
    russian: Option<String>,
    english: Option<String>,
    japanese: Option<String>,
    synonyms: Vec<String>,
    description: Option<String>,
    kind: Option<String>,
    status: Option<String>,
    score: Option<f32>,
    episodes: Option<i64>,
    next_episode_at: Option<String>,
    opengraph_image_url: Option<String>,
    mal_id: Option<IdValue>,
    aired_on: Option<IncompleteDate>,
    poster: Option<Poster>,
    genres: Vec<Genre>,
    studios: Vec<Studio>,
    videos: Vec<Video>,
    screenshots: Vec<ScreenshotNode>,
    related: Vec<RelatedNode>,
    character_roles: Vec<CharacterRole>,
    person_roles: Vec<PersonRole>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct AnimeData {
    animes: Vec<AnimeNode>,
}

#[derive(Debug, Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct GqlEnvelope<T> {
    data: Option<T>,
    errors: Option<Vec<GqlError>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry<T> {
    saved_at: u64,
    data: T,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RestImage { original: Option<String> }

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

fn is_rate_limited(error: &str) -> bool {
    error.contains("http status 429")
}

/// Retry every Shikimori HTTP operation after a rate-limit response. Other
/// failures are returned immediately; after three 429 responses the last
/// error is returned to the host.
fn retry_rate_limited<T, F>(mut request: F) -> Result<T, String>
where
    F: FnMut() -> Result<T, String>,
{
    for attempt in 0..RATE_LIMIT_ATTEMPTS {
        match request() {
            Ok(value) => return Ok(value),
            Err(error) if is_rate_limited(&error) && attempt + 1 < RATE_LIMIT_ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_millis(500 * (u64::from(attempt) + 1)));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("rate-limit retry loop always returns")
}

fn gql<T: DeserializeOwned>(query: &str, variables: serde_json::Value) -> Result<T, String> {
    let body = serde_json::json!({ "query": query, "variables": variables });
    let envelope: GqlEnvelope<T> = retry_rate_limited(|| anm_sdk::post_json(GRAPHQL, &body))?;
    if let Some(errors) = envelope.errors {
        if let Some(first) = errors.into_iter().next() {
            return Err(format!("shikimori graphql: {}", first.message));
        }
    }
    envelope
        .data
        .ok_or_else(|| "shikimori graphql: empty data".to_string())
}

fn fetch_node(id: &str) -> Result<AnimeNode, String> {
    let key = format!("shikimori:v3:anime:{id}");
    if let Ok(Some(cached)) = anm_sdk::data_cache_get::<CacheEntry<AnimeNode>>(&key) {
        if now_secs().saturating_sub(cached.saved_at) < CACHE_TTL_SECS {
            return Ok(cached.data);
        }
    }

    let data: AnimeData = gql(ANIME_QUERY, serde_json::json!({ "id": id }))?;
    let node = data
        .animes
        .into_iter()
        .next()
        .ok_or_else(|| format!("shikimori: anime '{id}' not found"))?;
    let _ = anm_sdk::cache_set(
        &key,
        &CacheEntry {
            saved_at: now_secs(),
            data: node.clone(),
        },
    );
    Ok(node)
}

fn person_alternative_names(name: &str, russian: Option<&str>) -> Vec<AlternativeName> {
    russian
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case(name.trim()))
        .map(|value| AlternativeName {
            name: value.to_string(),
            language: "ru".to_string(),
        })
        .into_iter()
        .collect()
}

fn character_role(role: &CharacterRole) -> Option<String> {
    let roles: Vec<String> = role.roles_en.iter().chain(role.roles_ru.iter()).map(|value| value.trim().to_lowercase()).collect();
    if roles.iter().any(|value| matches!(value.as_str(), "main" | "главный" | "главная" | "главные")) { Some("main".into()) }
    else if roles.iter().any(|value| matches!(value.as_str(), "supporting" | "второстепенный" | "второстепенная" | "второстепенные")) { Some("supporting".into()) }
    else { None }
}

fn character_records(id: &str) -> Result<Vec<exports::animex::module::tracker_character_roles_api::CharacterWithRole>, String> {
    let node = fetch_node(id)?;
    Ok(node.character_roles.into_iter().map(|role| {
        let importance = character_role(&role);
        exports::animex::module::tracker_character_roles_api::CharacterWithRole {
            role: importance,
            character: Character {
                tracker_data: character_tracker_data(&role.character),
                id: role.character.id,
                alternative_names: person_alternative_names(&role.character.name, role.character.russian.as_deref()),
                name: role.character.name, image: poster_url(role.character.poster.as_ref()), seiyuu: Vec::new(),
            },
        }
    }).collect())
}

impl exports::animex::module::tracker_character_roles_api::Guest for Component {
    fn get_characters(id: String) -> Result<Vec<exports::animex::module::tracker_character_roles_api::CharacterWithRole>, String> { character_records(&id) }
}

fn absolutize(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else if path.starts_with('/') {
        format!("{SITE}{path}")
    } else {
        format!("{SITE}/{path}")
    }
}

fn poster_url(poster: Option<&Poster>) -> Option<String> {
    let poster = poster?;
    poster
        .original_url
        .clone()
        .or_else(|| poster.preview_url.clone())
        .map(|path| absolutize(&path))
}

fn airing_status(value: Option<&str>) -> AiringStatus {
    match value.unwrap_or_default() {
        "anons" => AiringStatus::Anons,
        "ongoing" => AiringStatus::Ongoing,
        "released" => AiringStatus::Released,
        _ => AiringStatus::Unknown,
    }
}

fn video_kind(value: &str) -> VideoKind {
    match value {
        "op" => VideoKind::Opening,
        "ed" => VideoKind::Ending,
        "pv" => VideoKind::Pv,
        "character_trailer" => VideoKind::Trailer,
        _ => VideoKind::Other,
    }
}

fn preview(node: &AnimeNode) -> AnimePreview {
    let year = node
        .aired_on
        .as_ref()
        .and_then(|date| date.year)
        .and_then(|value| u16::try_from(value).ok());
    let mal = node
        .mal_id
        .as_ref()
        .map(IdValue::as_string)
        .filter(|value| !value.is_empty());

    let mut tracker_data = vec![TrackerData {
        name: "Shikimori".to_string(),
        data: node.id.clone(),
    }];
    if let Some(id) = mal {
        tracker_data.push(TrackerData {
            name: "MyAnimeList".to_string(),
            data: id,
        });
    }

    AnimePreview {
        id: node.id.clone(),
        title: node.name.clone(),
        alternative_names: alternative_names(node, &node.name),
        genres: release_genres(node),
        description: node.description.clone(),
        poster: poster_url(node.poster.as_ref()),
        year,
        score: node.score,
        kind: node
            .kind
            .as_deref()
            .and_then(normalize_anime_kind)
            .map(str::to_string),
        status: airing_status(node.status.as_deref()),
        episodes_count: node
            .episodes
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0),
        tracker_data,
        source_ref: None,
    }
}

fn normalize_anime_kind(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "tv" => Some("tv"),
        "movie" => Some("movie"),
        "ova" => Some("ova"),
        "ona" => Some("ona"),
        "special" => Some("special"),
        "tv_special" | "tv special" | "tvspecial" => Some("tv_special"),
        "music" => Some("music"),
        "pv" => Some("pv"),
        "cm" => Some("cm"),
        _ => None,
    }
}

#[cfg(test)]
mod anime_kind_tests {
    use super::normalize_anime_kind;

    #[test]
    fn only_canonical_anime_kinds_are_emitted() {
        assert_eq!(normalize_anime_kind("TV"), Some("tv"));
        assert_eq!(normalize_anime_kind("TV Special"), Some("tv_special"));
        assert_eq!(normalize_anime_kind("ONA"), Some("ona"));
        assert_eq!(normalize_anime_kind("unknown"), None);
    }
}

fn studio_details(studio: &Studio) -> animex::module::types::Studio {
    let id = studio.id.as_deref().filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()));
    animex::module::types::Studio {
        url: id.map(|id| format!("{SITE}/animes/studio/{id}")),
        id: id.map(str::to_owned),
        image_url: studio.image_url.as_deref().filter(|url| !url.trim().is_empty()).map(absolutize),
        name: (!studio.name.trim().is_empty()).then(|| studio.name.clone()),
    }
}

fn release(node: &AnimeNode) -> Release {
    let preview = preview(node);
    let name = node
        .russian
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(node.name.as_str())
        .to_string();
    Release {
        studio_details: Some(node.studios.iter().map(studio_details).collect()), episode_duration_seconds: None, aired_from: None, aired_to: None, regional_premieres: vec![], age_rating: None, original_source: None, score_scale: None, url: None,

        id: node.id.clone(),
        name: name.clone(),
        alternative_names: alternative_names(node, &name),
        tracker_data: preview.tracker_data.clone(),
        other_info: Vec::new(),
        banner_url: node
            .opengraph_image_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(absolutize),
        poster: preview.poster.clone(),
        description: node.description.clone(),
        genres: release_metadata::genres(preview.genres.clone()),
        studios: node
            .studios
            .iter()
            .map(|studio| studio.name.clone())
            .collect(),
        status: preview.status,
        kind: preview.kind.clone(),
        year: preview.year,
        trailer: node
            .videos
            .iter()
            .find(|video| video.kind == "pv")
            .map(|video| video.url.clone()),
        total_episodes: node
            .episodes
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0),
        next_episode_at: node.next_episode_at.as_deref().and_then(normalize_utc),
        rating: node.score,
        source_ref: None,
    }
}

fn release_genres(node: &AnimeNode) -> Vec<String> {
    node.genres.iter()
        .map(|genre| genre.russian.clone().unwrap_or_else(|| genre.name.clone()))
        .collect()
}

fn alternative_names(node: &AnimeNode, main: &str) -> Vec<AlternativeName> {
    let mut candidates: Vec<(&str, &str)> = Vec::new();
    if let Some(value) = node.russian.as_deref() {
        candidates.push((value, "ru"));
    }
    if let Some(value) = node.english.as_deref() {
        candidates.push((value, "en"));
    }
    if let Some(value) = node.japanese.as_deref() {
        candidates.push((value, "ja"));
    }
    if !node.name.is_empty() {
        candidates.push((node.name.as_str(), "unknown"));
    }
    for synonym in &node.synonyms {
        candidates.push((synonym.as_str(), "unknown"));
    }

    let mut out: Vec<AlternativeName> = Vec::new();
    for (value, language) in candidates {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.eq_ignore_ascii_case(main)
            || out
                .iter()
                .any(|known| known.name.eq_ignore_ascii_case(value))
        {
            continue;
        }
        out.push(AlternativeName {
            name: value.to_string(),
            language: language.to_string(),
        });
    }
    out
}

/// Normalize an ISO8601 timestamp with an offset into RFC3339 UTC.
fn normalize_utc(value: &str) -> Option<String> {
    use chrono::{DateTime, SecondsFormat, Utc};
    let parsed = DateTime::parse_from_rfc3339(value.trim()).ok()?;
    Some(
        parsed
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

struct Component;

impl Guest for Component {
    fn search(query: String, limit: u32) -> Result<Vec<AnimePreview>, String> {
        let limit = limit.clamp(1, 50);
        let data: AnimeData = gql(
            SEARCH_QUERY,
            serde_json::json!({ "q": query, "limit": limit }),
        )?;
        Ok(data.animes.iter().map(preview).collect())
    }

    fn get_anime(id: String) -> Result<Release, String> {
        let node = fetch_node(&id)?;
        Ok(release(&node))
    }

    fn get_characters(id: String) -> Result<Vec<Character>, String> {
        Ok(character_records(&id)?.into_iter().map(|entry| entry.character).collect())
    }

    fn get_staff(id: String) -> Result<Vec<StaffMember>, String> {
        let node = fetch_node(&id)?;
        Ok(node
            .person_roles
            .into_iter()
            .map(|role| StaffMember {
                id: role.person.id,
                alternative_names: person_alternative_names(
                    &role.person.name,
                    role.person.russian.as_deref(),
                ),
                name: role.person.name,
                role: role.roles_ru.join(", "),
                image: poster_url(role.person.poster.as_ref()),
            })
            .collect())
    }

    fn get_videos(id: String) -> Result<Vec<TrackerVideo>, String> {
        let node = fetch_node(&id)?;
        Ok(node
            .videos
            .into_iter()
            .map(|video| TrackerVideo {
                kind: video_kind(&video.kind),
                id: video.id,
                name: video.name,
                url: video.url,
                thumbnail: Some(absolutize(&video.image_url)),
            })
            .collect())
    }

    fn get_screenshots(id: String) -> Result<Vec<Screenshot>, String> {
        let node = fetch_node(&id)?;
        Ok(node
            .screenshots
            .into_iter()
            .map(|screenshot| Screenshot {
                original: absolutize(&screenshot.original_url),
                preview: Some(absolutize(&screenshot.x332_url)),
            })
            .collect())
    }
}

fn character_tracker_data(node: &CharacterNode) -> Vec<TrackerData> {
    let mut links = vec![TrackerData { name: "Shikimori".into(), data: node.id.clone() }];
    if let Some(id) = node.mal_id.as_ref().map(IdValue::as_string).filter(|id| !id.is_empty() && id != "0") {
        links.push(TrackerData { name: "MyAnimeList".into(), data: id });
    }
    links
}

#[cfg(test)]
mod character_link_tests {
    use super::*;
    #[test]
    fn character_links_keep_explicit_mal_identity_and_tolerate_missing_links() {
        let node = CharacterNode { id: "42".into(), mal_id: Some(IdValue::Num(20)), ..Default::default() };
        let links = character_tracker_data(&node);
        assert_eq!(links[0].name, "Shikimori");
        assert_eq!(links[0].data, "42");
        assert_eq!(links[1].name, "MyAnimeList");
        assert_eq!(links[1].data, "20");
        assert_eq!(character_tracker_data(&CharacterNode { id: "42".into(), ..Default::default() }).len(), 1);
    }
}

fn title_card(node: RelatedAnimeNode, manga: bool) -> TitleCard {
    let name = node
        .russian
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&node.name)
        .to_string();
    let id = if manga {
        format!("manga:{}", node.id)
    } else {
        node.id.clone()
    };
    TitleCard {
        id: id.clone(),
        name: name.clone(),
        alternative_names: if name != node.name {
            vec![AlternativeName {
                name: node.name,
                language: "unknown".into(),
            }]
        } else {
            vec![]
        },
        poster: poster_url(node.poster.as_ref()),
        media_type: if manga {
            MediaType::Manga
        } else {
            MediaType::Anime
        },
        format: node.kind,
        status: airing_status(node.status.as_deref()),
        year: node
            .aired_on
            .and_then(|d| d.year)
            .and_then(|y| u16::try_from(y).ok()),
        tracker_data: vec![TrackerData {
            name: "Shikimori".into(),
            data: id,
        }],
        source_ref: None,
        url: Some(format!(
            "{SITE}/{}/{}",
            if manga { "mangas" } else { "animes" },
            node.id
        )),
    }
}

impl exports::animex::module::related_titles_api::Guest for Component {
    fn get_related(release: ReleaseRef) -> Result<Vec<RelatedTitle>, String> {
        let node = fetch_node(&release.id)?;
        Ok(node
            .related
            .into_iter()
            .filter_map(|r| {
                let (node, manga) = match (r.anime, r.manga) {
                    (Some(a), _) => (a, false),
                    (_, Some(m)) => (m, true),
                    _ => return None,
                };
                Some(RelatedTitle {
                    title: title_card(node, manga),
                    relation: TitleRelation {
                        code: r.relation_kind,
                        label: r.relation_text,
                    },
                })
            })
            .collect())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SimilarNode {
    id: IdValue,
    name: String,
    russian: Option<String>,
    image: Option<RestImage>,
    kind: Option<String>,
    status: Option<String>,
    aired_on: Option<String>,
}
impl exports::animex::module::similar_titles_api::Guest for Component {
    fn get_similar(release: ReleaseRef) -> Result<Vec<SimilarTitle>, String> {
        if release.id.is_empty() || !release.id.bytes().all(|c| c.is_ascii_digit()) {
            return Err("shikimori: invalid anime id".into());
        }
        let key = format!("shikimori:v1:similar:{}", release.id);
        let cached = anm_sdk::data_cache_get::<CacheEntry<Vec<SimilarNode>>>(&key)
            .ok()
            .flatten()
            .filter(|c| now_secs().saturating_sub(c.saved_at) < CACHE_TTL_SECS);
        let nodes = if let Some(c) = cached {
            c.data
        } else {
            let nodes: Vec<SimilarNode> = retry_rate_limited(||
                anm_sdk::get_json(&format!("{SITE}/api/animes/{}/similar", release.id))
            )?;
            let _ = anm_sdk::cache_set(
                &key,
                &CacheEntry {
                    saved_at: now_secs(),
                    data: nodes.clone(),
                },
            );
            nodes
        };
        Ok(nodes
            .into_iter()
            .map(|n| {
                let year = n
                    .aired_on
                    .as_deref()
                    .and_then(|d| d.get(..4))
                    .and_then(|y| y.parse().ok());
                let poster = n.image.and_then(|i| i.original).map(|p| Poster {
                    original_url: Some(absolutize(&p)),
                    preview_url: None,
                });
                SimilarTitle {
                    title: title_card(
                        RelatedAnimeNode {
                            id: n.id.as_string(),
                            name: n.name,
                            russian: n.russian,
                            poster,
                            kind: n.kind,
                            status: n.status,
                            aired_on: Some(IncompleteDate { year }),
                        },
                        false,
                    ),
                    reason: None,
                }
            })
            .collect())
    }
}

anm_sdk::release_metadata!();

impl exports::animex::module::release_details_api::Guest for Component {
    fn get_release_details(reference: ReleaseRef) -> Result<crate::animex::module::types::Release, String> {
        anime_details::get(&reference.id)
    }
}

export!(Component);

#[cfg(test)]
mod release_tests {
    #[test]
    fn related_cards_keep_media_identity_and_display_metadata() {
        use super::*;
        let node = RelatedAnimeNode {
            id: "20".into(),
            name: "Original".into(),
            russian: Some("Название".into()),
            kind: Some("manga".into()),
            status: Some("released".into()),
            aired_on: Some(IncompleteDate { year: Some(2000) }),
            poster: Some(Poster {
                original_url: Some("/poster.jpg".into()),
                preview_url: None,
            }),
        };
        let anime = title_card(node.clone(), false);
        let manga = title_card(node, true);
        assert_eq!(anime.id, "20");
        assert_eq!(manga.id, "manga:20");
        assert!(matches!(manga.media_type, MediaType::Manga));
        assert_eq!(manga.name, "Название");
        assert_eq!(manga.alternative_names[0].name, "Original");
        assert_eq!(manga.year, Some(2000));
        assert_eq!(
            manga.poster.as_deref(),
            Some("https://shikimori.io/poster.jpg")
        );
        assert_eq!(manga.url.as_deref(), Some("https://shikimori.io/mangas/20"));
        assert_eq!(manga.tracker_data[0].data, "manga:20");
    }

    #[test]
    fn similar_rest_payload_accepts_numeric_ids_and_missing_optional_metadata() {
        let nodes: Vec<super::SimilarNode> = serde_json::from_str(
            r#"[{"id":20,"name":"Original","russian":null,"image":{"original":"/poster.jpg"},"kind":"tv","status":"released","aired_on":"2002-10-03"},{"id":"21","name":"Other"}]"#
        ).unwrap();
        assert_eq!(nodes[0].id.as_string(), "20");
        assert_eq!(nodes[1].id.as_string(), "21");
        assert!(nodes[1].image.is_none());
        assert!(nodes[1].aired_on.is_none());
    }

    use super::{normalize_utc, release, AnimeNode};

    #[test]
    fn person_names_use_language_tagged_alternatives_without_empty_or_duplicate_names() {
        let names = super::person_alternative_names("Original", Some("  Русское имя  "));
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].name, "Русское имя");
        assert_eq!(names[0].language, "ru");
        for russian in [None, Some(""), Some("  "), Some(" original ")] {
            assert!(super::person_alternative_names("Original", russian).is_empty());
        }
    }

    #[test]
    fn character_roles_use_provider_values_and_do_not_guess_missing_roles() {
        for (raw, expected) in [("Main", Some("main")), ("Supporting", Some("supporting")), ("Unknown", None), ("", None)] {
            let role = super::CharacterRole { roles_en: vec![raw.into()], ..Default::default() };
            assert_eq!(super::character_role(&role).as_deref(), expected);
        }
    }

    #[test]
    fn search_preview_exposes_own_tracker_and_optional_mal_link() {
        for mal in [
            None,
            Some(super::IdValue::Num(123)),
            Some(super::IdValue::Text("123".into())),
        ] {
            let has_mal = mal.is_some();
            let node = AnimeNode {
                id: "20".into(),
                mal_id: mal,
                ..Default::default()
            };
            let preview = super::preview(&node);
            assert_eq!(preview.tracker_data[0].name, "Shikimori");
            assert_eq!(preview.tracker_data[0].data, "20");
            assert_eq!(preview.tracker_data.len(), if has_mal { 2 } else { 1 });
            if has_mal {
                assert_eq!(preview.tracker_data[1].name, "MyAnimeList");
                assert_eq!(preview.tracker_data[1].data, "123");
            }
        }
    }

    #[test]
    fn release_maps_shikimori_contract_fields() {
        let node = AnimeNode {
            id: "20".into(),
            name: "Naruto".into(),
            russian: Some("Наруто".into()),
            english: Some("Naruto".into()),
            japanese: Some("ナルト".into()),
            synonyms: vec!["NARUTO".into()],
            description: Some("Synopsis".into()),
            genres: vec![super::Genre { name: "Action".into(), russian: Some("Боевик".into()) }],
            score: Some(7.9),
            episodes: Some(220),
            next_episode_at: Some("2026-10-29T18:00:00+03:00".into()),
            opengraph_image_url: Some("/system/animes/original/20.jpg".into()),
            ..Default::default()
        };
        let release = release(&node);
        let preview = super::preview(&node);
        assert!(preview.alternative_names.iter().any(|name| name.name == "Наруто" && name.language == "ru"));
        assert!(preview.alternative_names.iter().any(|name| name.name == "ナルト" && name.language == "ja"));
        assert!(preview.alternative_names.iter().all(|name| !name.name.eq_ignore_ascii_case(&preview.title)));
        assert_eq!(preview.description, release.description);
        assert_eq!(preview.genres, vec!["Боевик"]);
        assert_eq!(preview.genres, release.genres.iter().map(|genre| genre.name.as_str()).collect::<Vec<_>>());
        assert_eq!(release.id, "20");
        assert_eq!(release.name, "Наруто");
        assert!(release
            .alternative_names
            .iter()
            .any(|name| name.name == "Naruto" && name.language == "en"));
        assert!(release
            .alternative_names
            .iter()
            .any(|name| name.name == "ナルト" && name.language == "ja"));
        assert_eq!(release.tracker_data[0].name, "Shikimori");
        assert_eq!(release.tracker_data[0].data, "20");
        assert_eq!(release.total_episodes, Some(220));
        assert_eq!(
            release.next_episode_at.as_deref(),
            Some("2026-10-29T15:00:00Z")
        );
        assert_eq!(release.rating, Some(7.9));
        assert_eq!(
            release.banner_url.as_deref(),
            Some("https://shikimori.io/system/animes/original/20.jpg")
        );
    }

    #[test]
    fn utc_normalization_rejects_garbage() {
        assert_eq!(
            normalize_utc("2026-10-29T18:00:00+03:00").as_deref(),
            Some("2026-10-29T15:00:00Z")
        );
        assert_eq!(normalize_utc("not a date"), None);
    }
}

#[cfg(test)]
mod statistics_tests {
    use super::*;
    #[test]
    fn maps_score_distribution_and_six_list_statuses_without_invented_categories() {
        let raw: StatisticsNode = serde_json::from_value(serde_json::json!({
            "id": "20", "score": 7.92,
            "scoresStats": [{"score": 10, "count": 3}, {"score": 9, "count": 2}, {"score": 10, "count": 1}, {"score": 0, "count": 8}, {"score": 11, "count": 8}, {"score": 8, "count": -1}],
            "statusesStats": [
                {"status":"watching","count":136}, {"status":"planned","count":367},
                {"status":"completed","count":1}, {"status":"rewatching","count":1},
                {"status":"on_hold","count":11}, {"status":"dropped","count":15}
            ]
        })).unwrap();
        let result = map_statistics(raw);
        assert_eq!(result.rating, Some(7.92));
        assert_eq!(result.score_scale, 10);
        assert_eq!(result.scores.len(), 2);
        assert_eq!(
            result.scores.iter().find(|s| s.score == 10).unwrap().count,
            4
        );
        assert_eq!(result.statuses.len(), 6);
        assert_eq!(
            result
                .statuses
                .iter()
                .find(|s| s.status == "on_hold")
                .unwrap()
                .label,
            "Отложено"
        );
        assert!(!result
            .statuses
            .iter()
            .any(|s| s.status == "favorite" || s.status == "other"));
    }
    #[test]
    fn empty_and_null_statistics_remain_supported_empty_data() {
        for value in [
            serde_json::json!({"id":"1"}),
            serde_json::json!({"id":"1","score":0,"scoresStats":null,"statusesStats":null}),
        ] {
            let result = map_statistics(serde_json::from_value(value).unwrap());
            assert!(result.rating.is_none());
            assert!(result.scores.is_empty());
            assert!(result.statuses.is_empty());
            assert_eq!(result.score_scale, 10);
        }
    }
}
