//! First-party source module: AniLibria (`org.anilibria.source`).
//!
//! Implements the WIT `source-api` interface on top of the public
//! `https://anilibria.top/api/v1` HTTP API. All network access goes through
//! `anm_sdk`, so the host enforces the domain allowlist and the default
//! User-Agent.

wit_bindgen::generate!({
    path: "../../vendor/sdk/wit",
    world: "source-module-catalog",
});

use std::cmp::Ordering;
use std::collections::HashSet;

use animex::module::types::{
    AiringStatus, AlternativeName, AnimePreview, CatalogPage, CatalogParams, Episode, EpisodeRef,
    FilterField, FilterMultiselect, FilterSelect, InfoBlock, OptionPair, PageMeta, PlayerCandidate,
    Release, ReleaseRef, TimeSkip, TimeSkipType, TrackerData,
};
use anm_sdk::protocol::anilibria_hls;
use exports::animex::module::source_api::Guest;
use serde::Deserialize;
use url::Url;

const API_BASE: &str = "https://anilibria.top/api/v1";
const SITE_BASE: &str = "https://anilibria.top";
/// Titles per page, both for the API and for the locally paginated `popular`.
const PAGE_SIZE: u32 = 25;
/// AniLibria has no server-side rating sort (`f[sorting]` is ignored), so the
/// "popular" feed is built from the first three catalog pages (ADR-8).
const POPULAR_SOURCE_PAGES: u32 = 3;

// ---------------------------------------------------------------------------
// HTTP response models (snake_case, matching AniLibria JSON).
// ---------------------------------------------------------------------------

/// AniLibria sometimes returns numeric ids and sometimes strings; accept both.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum IdValue {
    Num(i64),
    Text(String),
}

impl IdValue {
    fn as_string(&self) -> String {
        match self {
            IdValue::Num(n) => n.to_string(),
            IdValue::Text(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct LabeledValue {
    #[serde(default, deserialize_with = "deserialize_labeled_value")]
    value: Option<String>,
    description: Option<String>,
}

// String enums (type/season/role) and numeric publish_day share this API shape.
fn deserialize_labeled_value<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<IdValue>::deserialize(deserializer).map(|value| value.map(|value| value.as_string()))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawName {
    main: Option<String>,
    english: Option<String>,
    alternative: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawImageVariant {
    preview: Option<String>,
    src: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawImageSet {
    preview: Option<String>,
    src: Option<String>,
    optimized: Option<RawImageVariant>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawShikimori {
    id: Option<IdValue>,
    rating: Option<f32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawMal {
    id: Option<IdValue>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawRating {
    average: Option<f32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawAgeRating {
    label: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawGenre {
    id: Option<IdValue>,
    name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawMember {
    role: Option<LabeledValue>,
    nickname: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawTimeRange {
    start: Option<f32>,
    stop: Option<f32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawEpisode {
    id: Option<IdValue>,
    name: Option<String>,
    ordinal: Option<u32>,
    opening: Option<RawTimeRange>,
    ending: Option<RawTimeRange>,
    preview: Option<RawImageSet>,
    hls_480: Option<String>,
    hls_720: Option<String>,
    hls_1080: Option<String>,
    duration: Option<f32>,
}

/// Catalog item, release details and the genre list item all share this shape
/// (the details endpoint simply adds `genres`, `members` and `episodes`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawRelease {
    id: Option<IdValue>,
    #[serde(rename = "type")]
    release_type: Option<LabeledValue>,
    year: Option<u16>,
    name: Option<RawName>,
    season: Option<LabeledValue>,
    shikimori: Option<RawShikimori>,
    mal: Option<RawMal>,
    rating: Option<RawRating>,
    age_rating: Option<RawAgeRating>,
    publish_day: Option<LabeledValue>,
    poster: Option<RawImageSet>,
    background_covers: Vec<RawImageSet>,
    is_ongoing: bool,
    description: Option<String>,
    episodes_total: Option<u32>,
    next_episode_at: Option<String>,
    genres: Vec<RawGenre>,
    members: Vec<RawMember>,
    episodes: Vec<RawEpisode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct PaginationLinks {
    next: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Pagination {
    total: Option<u32>,
    count: Option<u32>,
    per_page: Option<u32>,
    current_page: Option<u32>,
    total_pages: Option<u32>,
    links: Option<PaginationLinks>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct CatalogMeta {
    pagination: Option<Pagination>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct CatalogResponse {
    data: Vec<RawRelease>,
    meta: Option<CatalogMeta>,
}

// ---------------------------------------------------------------------------
// Guest implementation.
// ---------------------------------------------------------------------------

struct Component;

impl exports::animex::module::filter_ui_api::Guest for Component {
    fn get_filter_ui(
    ) -> Result<Vec<exports::animex::module::filter_ui_api::FilterPresentation>, String> {
        use exports::animex::module::filter_ui_api::{ChoicePresentation, FilterPresentation};
        Ok(vec![
            FilterPresentation {
                id: "genre".into(),
                presentation: ChoicePresentation::Dropdown,
            },
            FilterPresentation {
                id: "type".into(),
                presentation: ChoicePresentation::RadioSelect,
            },
            FilterPresentation {
                id: "season".into(),
                presentation: ChoicePresentation::RadioSelect,
            },
        ])
    }
}

impl exports::animex::module::catalog_sorting_api::Guest for Component {
    fn get_sorting(
    ) -> Result<Option<exports::animex::module::catalog_sorting_api::SortingConfig>, String> {
        // The current AniLibria catalog endpoint ignores sorting parameters.
        Ok(None)
    }
}

impl Guest for Component {
    fn get_filters() -> Result<Vec<FilterField>, String> {
        let url = format!("{API_BASE}/anime/genres");
        let genres: Vec<RawGenre> = anm_sdk::get_json(&url)?;

        let genre_options: Vec<OptionPair> = genres
            .iter()
            .filter_map(|genre| {
                let value = genre.id.as_ref()?.as_string();
                let label = genre.name.as_deref()?.trim();
                if label.is_empty() {
                    None
                } else {
                    Some(OptionPair {
                        value,
                        label: label.to_string(),
                    })
                }
            })
            .collect();

        Ok(vec![
            FilterField::Multiselect(FilterMultiselect {
                id: "genre".to_string(),
                label: "Жанры".to_string(),
                options: genre_options,
                default: Vec::new(),
            }),
            FilterField::Select(FilterSelect {
                id: "type".to_string(),
                label: "Тип".to_string(),
                options: labeled_options(&[
                    ("TV", "ТВ-сериал"),
                    ("Movie", "Фильм"),
                    ("OVA", "OVA"),
                    ("ONA", "ONA"),
                    ("Special", "Спешл"),
                ]),
                default: None,
            }),
            FilterField::Select(FilterSelect {
                id: "season".to_string(),
                label: "Сезон".to_string(),
                options: labeled_options(&[
                    ("winter", "Зима"),
                    ("spring", "Весна"),
                    ("summer", "Лето"),
                    ("autumn", "Осень"),
                ]),
                default: None,
            }),
        ])
    }

    fn get_catalog(params: CatalogParams) -> Result<CatalogPage, String> {
        let search = effective_search(None, &params);
        fetch_catalog(params.page, search, &params.filters)
    }

    fn get_latest(page: u32) -> Result<CatalogPage, String> {
        fetch_catalog(page, None, &[])
    }

    fn get_popular(page: u32) -> Result<CatalogPage, String> {
        let page = page.max(1);
        let mut seen: HashSet<String> = HashSet::new();
        let mut items: Vec<AnimePreview> = Vec::new();

        for api_page in 1..=POPULAR_SOURCE_PAGES {
            let url = build_catalog_url(api_page, None, &[])?;
            let response: CatalogResponse = anm_sdk::get_json(&url)?;
            for raw in &response.data {
                if let Some(preview) = map_preview(raw) {
                    if seen.insert(preview.id.clone()) {
                        items.push(preview);
                    }
                }
            }
        }

        // Highest score first; titles without a score go to the end.
        items.sort_by(|a, b| match (a.score, b.score) {
            (Some(left), Some(right)) => right.total_cmp(&left),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        });

        let total = items.len() as u32;
        let total_pages = total.div_ceil(PAGE_SIZE);
        let start = page.saturating_sub(1).saturating_mul(PAGE_SIZE) as usize;
        let page_items: Vec<AnimePreview> = items
            .into_iter()
            .skip(start)
            .take(PAGE_SIZE as usize)
            .collect();

        Ok(CatalogPage {
            items: page_items,
            meta: PageMeta {
                page,
                per_page: PAGE_SIZE,
                total: Some(total),
                total_pages: Some(total_pages),
                has_next: page < total_pages,
            },
        })
    }

    fn search(query: String, params: CatalogParams) -> Result<CatalogPage, String> {
        let search = effective_search(Some(query.as_str()), &params);
        fetch_catalog(params.page, search, &params.filters)
    }

    fn get_release(release: ReleaseRef) -> Result<Release, String> {
        if release.id.trim().is_empty() {
            return Err("release id must not be empty".into());
        }
        let url = release_url(&release.id)?;
        let raw: RawRelease = anm_sdk::get_json(&url)?;
        Ok(map_release(&raw, &release.id))
    }

    fn get_episodes(release: ReleaseRef) -> Result<Vec<Episode>, String> {
        let url = release_url(&release.id)?;
        let raw: RawRelease = anm_sdk::get_json(&url)?;
        Ok(raw
            .episodes
            .iter()
            .enumerate()
            .map(|(index, episode)| map_episode(episode, index, &release.id))
            .collect())
    }

    fn get_players(
        release: ReleaseRef,
        episode: EpisodeRef,
    ) -> Result<Vec<PlayerCandidate>, String> {
        // The release mapping already put the protocol payload into
        // `episode.source-ref`, so the normal path needs no network call.
        if let Some(payload) = episode.payload.as_deref() {
            if let Ok(parsed) = serde_json::from_str::<anilibria_hls::Payload>(payload) {
                if !parsed.qualities.is_empty() {
                    return Ok(vec![PlayerCandidate {
                        protocol: anilibria_hls::PROTOCOL.to_string(),
                        payload: payload.to_string(),
                        label: Some("AniLibria".to_string()),
                        priority: Some(0),
                    }]);
                }
            }
        }

        // Fallback for hosts that only pass stable ids.
        let url = release_url(&release.id)?;
        let raw: RawRelease = anm_sdk::get_json(&url)?;
        let Some(raw_episode) = raw
            .episodes
            .iter()
            .enumerate()
            .find(|(index, candidate)| episode_id(candidate, *index, &release.id) == episode.id)
            .map(|(_, candidate)| candidate)
        else {
            return Ok(Vec::new());
        };
        match player_candidate(raw_episode, &episode.id)? {
            Some(candidate) => Ok(vec![candidate]),
            None => Ok(Vec::new()),
        }
    }
}

export!(Component);

// ---------------------------------------------------------------------------
// Request building.
// ---------------------------------------------------------------------------

fn build_catalog_url(
    page: u32,
    search: Option<&str>,
    filters: &[(String, String)],
) -> Result<String, String> {
    let mut url = Url::parse(&format!("{API_BASE}/anime/catalog/releases"))
        .map_err(|e| format!("build catalog url: {e}"))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("limit", &PAGE_SIZE.to_string());
        query.append_pair("page", &page.max(1).to_string());

        if let Some(term) = search {
            let term = term.trim();
            if !term.is_empty() {
                query.append_pair("f[search]", term);
            }
        }

        for (key, value) in filters {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                // Multiselect values arrive as separate tuples; the API accepts
                // repeated `f[genres]` parameters.
                "genre" | "genres" => {
                    query.append_pair("f[genres]", value);
                }
                "type" | "types" => {
                    query.append_pair("f[types]", value);
                }
                "season" | "seasons" => {
                    query.append_pair("f[seasons]", value);
                }
                // The free-text filter is handled through `effective_search`.
                "q" | "search" => {}
                _ => {}
            }
        }
    }
    Ok(url.to_string())
}

fn release_url(id: &str) -> Result<String, String> {
    let mut url = Url::parse(&format!("{API_BASE}/anime/releases"))
        .map_err(|e| format!("build release url: {e}"))?;
    url.path_segments_mut()
        .map_err(|_| "release url cannot be a base".to_string())?
        .push(id);
    Ok(url.to_string())
}

/// Search term precedence: explicit argument → `CatalogParams.search` →
/// `q` filter. An empty explicit term falls through to the catalog.
fn effective_search<'a>(explicit: Option<&'a str>, params: &'a CatalogParams) -> Option<&'a str> {
    let explicit = explicit.map(str::trim).filter(|term| !term.is_empty());
    explicit
        .or_else(|| {
            params
                .search
                .as_deref()
                .map(str::trim)
                .filter(|term| !term.is_empty())
        })
        .or_else(|| {
            params
                .filters
                .iter()
                .find(|(key, _)| key == "q" || key == "search")
                .map(|(_, value)| value.trim())
                .filter(|term| !term.is_empty())
        })
}

// ---------------------------------------------------------------------------
// Catalog mapping.
// ---------------------------------------------------------------------------

fn fetch_catalog(
    page: u32,
    search: Option<&str>,
    filters: &[(String, String)],
) -> Result<CatalogPage, String> {
    let page = page.max(1);
    let url = build_catalog_url(page, search, filters)?;
    let response: CatalogResponse = anm_sdk::get_json(&url)?;

    let items: Vec<AnimePreview> = response.data.iter().filter_map(map_preview).collect();
    let pagination = response
        .meta
        .as_ref()
        .and_then(|meta| meta.pagination.as_ref());
    let meta = page_meta(page, pagination);

    Ok(CatalogPage { items, meta })
}

fn page_meta(page: u32, pagination: Option<&Pagination>) -> PageMeta {
    let current_page = pagination.and_then(|p| p.current_page).unwrap_or(page);
    let per_page = pagination.and_then(|p| p.per_page).unwrap_or(PAGE_SIZE);
    let total = pagination.and_then(|p| p.total);
    let total_pages = pagination.and_then(|p| p.total_pages);
    let has_next = pagination
        .and_then(|p| p.links.as_ref())
        .and_then(|links| links.next.as_ref())
        .is_some()
        || total_pages
            .map(|pages| current_page < pages)
            .unwrap_or(false);

    PageMeta {
        page: current_page,
        per_page,
        total,
        total_pages,
        has_next,
    }
}

fn map_preview(raw: &RawRelease) -> Option<AnimePreview> {
    let id = raw.id.as_ref()?.as_string();
    Some(preview_of(raw, &id))
}

fn preview_of(raw: &RawRelease, id: &str) -> AnimePreview {
    let title = raw
        .name
        .as_ref()
        .and_then(|name| {
            first_non_empty([
                name.main.as_deref(),
                name.english.as_deref(),
                name.alternative.as_deref(),
            ])
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("#{id}"));

    let score = raw
        .shikimori
        .as_ref()
        .and_then(|shikimori| shikimori.rating)
        .or_else(|| raw.rating.as_ref().and_then(|rating| rating.average));

    let kind = raw
        .release_type
        .as_ref()
        .and_then(|value| value.value.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(normalize_kind)
        .map(str::to_string);

    let episodes_count = raw.episodes_total.or_else(|| {
        if raw.episodes.is_empty() {
            None
        } else {
            Some(raw.episodes.len() as u32)
        }
    });

    AnimePreview {
        id: id.to_string(),
        alternative_names: alternative_names(raw, &title),
        title,
        genres: release_genres(raw),
        description: release_description(raw),
        poster: pick_image(raw.poster.as_ref()),
        year: raw.year,
        score,
        kind,
        status: if raw.is_ongoing {
            AiringStatus::Ongoing
        } else {
            AiringStatus::Released
        },
        episodes_count,
        tracker_data: preview_tracker_data(raw),
        source_ref: None,
    }
}

fn normalize_kind(value: &str) -> Option<&'static str> {
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
mod tests {
    use super::{
        episode_payload, map_release, normalize_kind, player_candidate, time_skips, IdValue,
        LabeledValue, RawAgeRating, RawEpisode, RawMember, RawName, RawRating, RawRelease,
        RawShikimori, RawTimeRange, TimeSkipType,
    };
    use anm_sdk::protocol::anilibria_hls;

    #[test]
    fn previews_and_releases_expose_tracker_data_with_missing_and_multiple_trackers() {
        for (links, expected) in [
            (r#"{}"#, Vec::<(&str, &str)>::new()),
            (r#"{"mal":{"id":"123"}}"#, vec![("MyAnimeList", "123")]),
            (
                r#"{"shikimori":{"id":20},"mal":{"id":"123"}}"#,
                vec![("Shikimori", "20"), ("MyAnimeList", "123")],
            ),
        ] {
            let raw: RawRelease = serde_json::from_str(links).unwrap();
            let preview = super::preview_of(&raw, "source-id");
            let trackers: Vec<_> = preview
                .tracker_data
                .iter()
                .map(|item| (item.name.as_str(), item.data.as_str()))
                .collect();
            assert_eq!(trackers, expected);
            let release = map_release(&raw, "source-id");
            let release_trackers: Vec<_> = release
                .tracker_data
                .iter()
                .map(|item| (item.name.as_str(), item.data.as_str()))
                .collect();
            assert_eq!(release_trackers, expected);
        }
    }

    #[test]
    fn labeled_values_accept_strings_integers_null_and_missing() {
        for (json, expected) in [
            (r#"{"value":7,"description":"Воскресенье"}"#, Some("7")),
            (r#"{"value":"7"}"#, Some("7")),
            (r#"{"value":"TV"}"#, Some("TV")),
            (r#"{"value":"autumn"}"#, Some("autumn")),
            (r#"{"value":"voicing"}"#, Some("voicing")),
            (r#"{"value":null}"#, None),
            (r#"{}"#, None),
        ] {
            let value: LabeledValue = serde_json::from_str(json).unwrap();
            assert_eq!(value.value.as_deref(), expected, "{json}");
        }
        for invalid in [r#"{"value":true}"#, r#"{"value":{}}"#, r#"{"value":[]}"#] {
            assert!(serde_json::from_str::<LabeledValue>(invalid).is_err());
        }
    }

    #[test]
    fn catalog_accepts_numeric_publish_day_without_losing_release_metadata() {
        let response: super::CatalogResponse = serde_json::from_str(
            r#"{"data":[{
                "id":42,
                "name":{"main":"Тайтл"},
                "type":{"value":"TV","description":"ТВ-сериал"},
                "season":{"value":"autumn","description":"Осень"},
                "publish_day":{"value":7,"description":"Воскресенье"},
                "members":[{"role":{"value":"voicing"},"nickname":"Команда"}]
            }]}"#,
        )
        .unwrap();
        assert_eq!(response.data.len(), 1);
        let raw = &response.data[0];
        assert_eq!(
            raw.publish_day.as_ref().unwrap().value.as_deref(),
            Some("7")
        );
        let release = map_release(raw, "42");
        assert_eq!(release.name, "Тайтл");
        assert_eq!(release.kind.as_deref(), Some("tv"));
        assert!(release.other_info.iter().any(|block| {
            block.title == "День выхода" && block.data == vec!["Воскресенье"]
        }));
        assert!(release.other_info.iter().any(|block| {
            block.title == "Озвучка" && block.data == vec!["Команда"]
        }));
    }

    #[test]
    fn preview_metadata_matches_full_release() {
        let raw: RawRelease = serde_json::from_str(r#"{
            "id": 42,
            "name": {"main": "Тайтл", "english": "Title", "alternative": "別名"},
            "genres": [{"name": " Фэнтези "}, {"name": ""}],
            "description": "  Описание  "
        }"#).unwrap();
        let preview = super::preview_of(&raw, "42");
        let release = map_release(&raw, "42");
        assert_eq!(preview.alternative_names.len(), release.alternative_names.len());
        assert_eq!(preview.alternative_names[0].name, release.alternative_names[0].name);
        assert_eq!(preview.alternative_names[1].language, release.alternative_names[1].language);
        assert_eq!(preview.genres, vec!["Фэнтези"]);
        assert_eq!(preview.genres, release.genres);
        assert_eq!(preview.description.as_deref(), Some("Описание"));
        assert_eq!(preview.description, release.description);
        let missing = super::preview_of(&RawRelease::default(), "empty");
        assert!(missing.alternative_names.is_empty());
        assert!(missing.genres.is_empty());
        assert!(missing.description.is_none());
    }

    #[test]
    fn episode_payload_is_a_protocol_payload() {
        let raw = RawEpisode {
            id: Some(IdValue::Text("e1".into())),
            hls_1080: Some("https://cdn/1080.m3u8".into()),
            hls_720: Some("https://cdn/720.m3u8".into()),
            ..Default::default()
        };
        let payload = episode_payload(&raw, "e1").unwrap();
        let parsed: anilibria_hls::Payload = serde_json::from_str(&payload).unwrap();
        assert_eq!(parsed.qualities.len(), 2);
        assert!(parsed.translations.is_empty());
        assert!(serde_json::from_str::<serde_json::Value>(&payload).unwrap().get("translations").is_none());
        assert_eq!(parsed.best_url(), Some("https://cdn/1080.m3u8"));

        let candidate = player_candidate(&raw, "e1").unwrap().unwrap();
        assert_eq!(candidate.protocol, anilibria_hls::PROTOCOL);
    }

    #[test]
    fn release_maps_names_trackers_and_info() {
        let raw = RawRelease {
            id: Some(IdValue::Num(42)),
            name: Some(RawName {
                main: Some("Издалека".into()),
                english: Some("Kanata kara".into()),
                alternative: Some("カナタカラ".into()),
            }),
            shikimori: Some(RawShikimori {
                id: Some(IdValue::Num(63753)),
                rating: None,
            }),
            season: Some(LabeledValue {
                value: Some("autumn".into()),
                description: Some("Осень".into()),
            }),
            age_rating: Some(RawAgeRating {
                label: Some("16+".into()),
            }),
            members: vec![RawMember {
                role: Some(LabeledValue {
                    value: Some("timing".into()),
                    description: Some("Работа со звуком".into()),
                }),
                nickname: Some("Хана".into()),
            }],
            rating: Some(RawRating { average: Some(8.1) }),
            ..Default::default()
        };
        let release = map_release(&raw, "42");
        assert_eq!(release.id, "42");
        assert_eq!(release.name, "Издалека");
        assert_eq!(release.alternative_names.len(), 2);
        assert_eq!(release.alternative_names[0].name, "Kanata kara");
        assert_eq!(release.alternative_names[0].language, "en");
        assert_eq!(release.alternative_names[1].language, "ja");
        assert_eq!(release.tracker_data[0].name, "Shikimori");
        assert_eq!(release.tracker_data[0].data, "63753");
        assert!(release
            .other_info
            .iter()
            .any(|block| block.title == "Работа со звуком"));
        assert!(release
            .other_info
            .iter()
            .any(|block| block.title == "Возрастной рейтинг"));
        assert!(release
            .other_info
            .iter()
            .any(|block| block.title == "Сезон"));
        assert_eq!(release.rating, Some(8.1));
    }

    #[test]
    fn episode_time_skips_include_opening_and_ending() {
        let raw = RawEpisode {
            opening: Some(RawTimeRange {
                start: Some(92.0),
                stop: Some(182.0),
            }),
            ending: Some(RawTimeRange {
                start: Some(1330.0),
                stop: Some(1420.0),
            }),
            ..Default::default()
        };
        let skips = time_skips(&raw);
        assert_eq!(skips.len(), 2);
        assert!(matches!(skips[0].kind, TimeSkipType::Opening));
        assert_eq!(skips[0].start, 92.0);
        assert_eq!(skips[0].end, 182.0);
        assert!(matches!(skips[1].kind, TimeSkipType::Ending));
        assert_eq!(skips[1].start, 1330.0);
        assert_eq!(skips[1].end, 1420.0);
    }

    #[test]
    fn kind_normalization_accepts_canonical_and_legacy_labels() {
        for (input, expected) in [
            ("TV", "tv"),
            ("Movie", "movie"),
            ("OVA", "ova"),
            ("ONA", "ona"),
            ("Special", "special"),
            ("TV Special", "tv_special"),
            ("TV_SPECIAL", "tv_special"),
            ("Music", "music"),
            ("PV", "pv"),
            ("CM", "cm"),
        ] {
            assert_eq!(normalize_kind(input), Some(expected));
        }
    }

    #[test]
    fn kind_normalization_drops_unknown_values() {
        assert_eq!(normalize_kind("series"), None);
        assert_eq!(normalize_kind(""), None);
    }
}

// ---------------------------------------------------------------------------
// Release details mapping.
// ---------------------------------------------------------------------------

fn map_release(raw: &RawRelease, requested_id: &str) -> Release {
    let release_id = raw
        .id
        .as_ref()
        .map(IdValue::as_string)
        .unwrap_or_else(|| requested_id.to_string());

    let preview = preview_of(raw, &release_id);
    let name = preview.title.clone();
    Release {
        id: release_id,
        name: name.clone(),
        alternative_names: alternative_names(raw, &name),
        tracker_data: tracker_data(raw),
        other_info: other_info(raw),
        banner_url: raw
            .background_covers
            .first()
            .and_then(|cover| pick_banner(Some(cover))),
        poster: preview.poster.clone(),
        description: preview.description.clone(),
        genres: preview.genres.clone(),
        studios: Vec::new(),
        status: preview.status,
        kind: preview.kind.clone(),
        year: preview.year,
        trailer: None,
        total_episodes: raw.episodes_total.or_else(|| {
            if raw.is_ongoing || raw.episodes.is_empty() {
                None
            } else {
                Some(raw.episodes.len() as u32)
            }
        }),
        next_episode_at: raw.next_episode_at.clone(),
        rating: raw
            .rating
            .as_ref()
            .and_then(|rating| rating.average)
            .filter(|value| *value > 0.0),
        source_ref: None,
    }
}

fn release_genres(raw: &RawRelease) -> Vec<String> {
    raw.genres.iter()
        .filter_map(|genre| genre.name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

fn release_description(raw: &RawRelease) -> Option<String> {
    raw.description.as_deref().map(str::trim)
        .filter(|value| !value.is_empty()).map(str::to_string)
}

fn alternative_names(raw: &RawRelease, main: &str) -> Vec<AlternativeName> {
    let Some(name) = raw.name.as_ref() else {
        return Vec::new();
    };
    let candidates = [
        (name.english.as_deref(), "en"),
        (name.alternative.as_deref(), ""),
    ];
    let mut out: Vec<AlternativeName> = Vec::new();
    for (value, language) in candidates {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        if value.eq_ignore_ascii_case(main)
            || out
                .iter()
                .any(|known| known.name.eq_ignore_ascii_case(value))
        {
            continue;
        }
        let language = if language.is_empty() {
            detect_language(value)
        } else {
            language
        };
        out.push(AlternativeName {
            name: value.to_string(),
            language: language.to_string(),
        });
    }
    out
}

/// Best-effort ISO 639-1 tag for a title the source did not label explicitly.
fn detect_language(value: &str) -> &'static str {
    let mut cyrillic = false;
    let mut kana = false;
    for ch in value.chars().filter(|ch| ch.is_alphabetic()) {
        let code = ch as u32;
        if (0x0400..=0x04FF).contains(&code) {
            cyrillic = true;
        } else if (0x3040..=0x30FF).contains(&code) {
            kana = true;
        }
    }
    // Latin script is ambiguous (English, romaji, transliteration), so we do
    // not guess `en` for unlabelled titles.
    if cyrillic {
        "ru"
    } else if kana {
        "ja"
    } else {
        "unknown"
    }
}

fn preview_tracker_data(raw: &RawRelease) -> Vec<TrackerData> {
    tracker_data(raw)
}

fn tracker_data(raw: &RawRelease) -> Vec<TrackerData> {
    [
        (
            "Shikimori",
            raw.shikimori.as_ref().and_then(|link| link.id.as_ref()),
        ),
        (
            "MyAnimeList",
            raw.mal.as_ref().and_then(|link| link.id.as_ref()),
        ),
    ]
    .into_iter()
    .filter_map(|(name, link)| {
        let data = link?.as_string();
        let data = data.trim();
        if data.is_empty() || data == "0" {
            return None;
        }
        Some(TrackerData {
            name: name.to_string(),
            data: data.to_string(),
        })
    })
    .collect()
}

fn other_info(raw: &RawRelease) -> Vec<InfoBlock> {
    let mut blocks: Vec<InfoBlock> = Vec::new();

    // All production roles, including individual voice actors, are credits.
    let mut roles: Vec<(String, Vec<String>)> = Vec::new();
    for member in &raw.members {
        let Some(role) = member.role.as_ref() else {
            continue;
        };
        let value = role.value.as_deref().map(str::trim).unwrap_or("");
        if value.is_empty() {
            continue;
        }
        let Some(nickname) = member
            .nickname
            .as_deref()
            .map(str::trim)
            .filter(|nickname| !nickname.is_empty())
        else {
            continue;
        };
        let title = role
            .description
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or(if value.eq_ignore_ascii_case("voicing") { "Озвучка" } else { value })
            .to_string();
        match roles.iter_mut().find(|(known, _)| known == &title) {
            Some((_, data)) => {
                if !data.iter().any(|known| known == nickname) {
                    data.push(nickname.to_string());
                }
            }
            None => roles.push((title, vec![nickname.to_string()])),
        }
    }
    for (title, data) in roles {
        blocks.push(InfoBlock { title, data });
    }

    if let Some(value) = raw
        .age_rating
        .as_ref()
        .and_then(|rating| rating.label.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        blocks.push(InfoBlock {
            title: "Возрастной рейтинг".to_string(),
            data: vec![value.to_string()],
        });
    }
    if let Some(value) = raw
        .season
        .as_ref()
        .and_then(|season| season.description.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        blocks.push(InfoBlock {
            title: "Сезон".to_string(),
            data: vec![value.to_string()],
        });
    }
    if let Some(value) = raw
        .publish_day
        .as_ref()
        .and_then(|day| day.description.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        blocks.push(InfoBlock {
            title: "День выхода".to_string(),
            data: vec![value.to_string()],
        });
    }
    blocks
}

fn map_episode(
    raw: &RawEpisode,
    index: usize,
    release_id: &str,
) -> Episode {
    let number = raw
        .ordinal
        .unwrap_or_else(|| (index as u32).saturating_add(1));
    let id = episode_id(raw, index, release_id);
    let source_ref = episode_payload(raw, &id).ok();

    Episode {
        id,
        number,
        title: raw
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        thumbnail: pick_image(raw.preview.as_ref()),
        duration: raw.duration,
        time_skips: time_skips(raw),
        source_ref,
    }
}

fn episode_id(raw: &RawEpisode, index: usize, release_id: &str) -> String {
    let number = raw
        .ordinal
        .unwrap_or_else(|| (index as u32).saturating_add(1));
    raw.id
        .as_ref()
        .map(IdValue::as_string)
        .unwrap_or_else(|| format!("{release_id}-{number}"))
}

/// Available HLS renditions, best first, as `(label, url)`.
fn episode_qualities(raw: &RawEpisode) -> Vec<(String, String)> {
    [
        ("1080p", raw.hls_1080.as_deref()),
        ("720p", raw.hls_720.as_deref()),
        ("480p", raw.hls_480.as_deref()),
    ]
    .into_iter()
    .filter_map(|(label, url)| {
        let url = url?.trim();
        if url.is_empty() {
            None
        } else {
            Some((label.to_string(), url.to_string()))
        }
    })
    .collect()
}

/// Build the protocol payload shared by `episode.source-ref` and
/// `get-players`. An empty quality list means "no playable rendition".
fn episode_payload(
    raw: &RawEpisode,
    episode_id: &str,
) -> Result<String, String> {
    let qualities = episode_qualities(raw)
        .into_iter()
        .map(|(label, url)| anilibria_hls::QualityRef { label, url })
        .collect::<Vec<_>>();
    if qualities.is_empty() {
        return Err("episode has no HLS renditions".into());
    }
    anm_sdk::protocol::build_payload(&anilibria_hls::Payload {
        episode_id: episode_id.to_string(),
        qualities,
        translations: Vec::new(),
        referer: Some(format!("{SITE_BASE}/")),
    })
}

fn player_candidate(
    raw: &RawEpisode,
    episode_id: &str,
) -> Result<Option<PlayerCandidate>, String> {
    if episode_qualities(raw).is_empty() {
        return Ok(None);
    }
    Ok(Some(PlayerCandidate {
        protocol: anilibria_hls::PROTOCOL.to_string(),
        payload: episode_payload(raw, episode_id)?,
        label: Some("AniLibria".to_string()),
        priority: Some(0),
    }))
}

fn time_skips(raw: &RawEpisode) -> Vec<TimeSkip> {
    let mut skips = Vec::new();
    if let Some((start, stop)) = range_values(raw.opening.as_ref()) {
        skips.push(TimeSkip {
            kind: TimeSkipType::Opening,
            start,
            end: stop,
        });
    }
    if let Some((start, stop)) = range_values(raw.ending.as_ref()) {
        skips.push(TimeSkip {
            kind: TimeSkipType::Ending,
            start,
            end: stop,
        });
    }
    skips
}

fn range_values(raw: Option<&RawTimeRange>) -> Option<(f32, f32)> {
    let raw = raw?;
    Some((raw.start?, raw.stop?))
}

// ---------------------------------------------------------------------------
// Small helpers.
// ---------------------------------------------------------------------------

fn labeled_options(pairs: &[(&str, &str)]) -> Vec<OptionPair> {
    pairs
        .iter()
        .map(|(value, label)| OptionPair {
            value: (*value).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}

fn first_non_empty<'a>(values: [Option<&'a str>; 3]) -> Option<&'a str> {
    values
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

/// Pick the optimized variant when present, otherwise the original, and make
/// the relative `/storage/...` path absolute.
fn pick_image(image: Option<&RawImageSet>) -> Option<String> {
    let image = image?;
    let path = image
        .optimized
        .as_ref()
        .and_then(|optimized| optimized.preview.as_deref().or(optimized.src.as_deref()))
        .or(image.preview.as_deref())
        .or(image.src.as_deref())?;
    normalize_image(path)
}

/// Prefer the full-size banner variant over the preview.
fn pick_banner(image: Option<&RawImageSet>) -> Option<String> {
    let image = image?;
    let path = image
        .optimized
        .as_ref()
        .and_then(|optimized| optimized.src.as_deref().or(optimized.preview.as_deref()))
        .or(image.src.as_deref())
        .or(image.preview.as_deref())?;
    normalize_image(path)
}

fn normalize_image(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    if path.starts_with("https://") || path.starts_with("http://") {
        Some(path.to_string())
    } else if path.starts_with('/') {
        Some(format!("{SITE_BASE}{path}"))
    } else {
        Some(format!("{SITE_BASE}/{path}"))
    }
}
