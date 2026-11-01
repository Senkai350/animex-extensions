//! AniList GraphQL tracker.
//!
//! The module deliberately contains no guest-side metadata cache. AniMex owns
//! the output cache, so refresh, persistence and provenance stay consistent
//! with every other source and tracker.

wit_bindgen::generate!({
    path: "../../vendor/sdk/wit",
    world: "tracker-module-episodes-character-roles",
});

use animex::module::types::{
    AiringStatus, AlternativeName, AnimePreview, Character, InfoBlock, MediaType, RelatedTitle, Release,
    ReleaseRef, ScoreCount, Screenshot, SimilarTitle, StaffMember, StaffRef, StatusCount,
    TitleCard, TitleRelation, TitleStatistics, TrackerData, TrackerEpisode, TrackerEpisodePage,
    TrackerVideo, VideoKind,
};
use exports::animex::module::tracker_api::Guest;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;

const API: &str = "https://graphql.anilist.co";
const SITE: &str = "https://anilist.co";
const SHIKIMORI_API: &str = "https://shikimori.io/api/graphql";

const SEARCH_QUERY: &str = r#"
query Search($search: String!, $perPage: Int!) {
  Page(page: 1, perPage: $perPage) {
    media(search: $search, type: ANIME, sort: SEARCH_MATCH) {
      id idMal type format status(version: 2)
      title { romaji english native userPreferred }
      description(asHtml: true)
      startDate { year month day }
      seasonYear episodes genres synonyms
      weightedScore meanScore
      coverImage { extraLarge large medium }
    }
  }
}
"#;

const DETAILS_QUERY: &str = r#"
query Media($id: Int!, $useAniList: Boolean!, $useMal: Boolean!) {
  byId: Media(id: $id, type: ANIME) @include(if: $useAniList) {
    __typename
    id idMal type format status(version: 2)
    title { romaji english native userPreferred }
    description(asHtml: true)
    startDate { year month day }
    endDate { year month day }
    season seasonYear episodes duration genres synonyms
    weightedScore meanScore popularity
    coverImage { extraLarge large medium }
    bannerImage
    trailer { id site thumbnail }
    nextAiringEpisode { id airingAt timeUntilAiring episode mediaId }
    studios(isMain: true) { nodes { name } }
    tags { name }
    siteUrl
    relations { edges { relationType node {
      id idMal type format status(version: 2)
      title { romaji english native userPreferred }
      startDate { year month day }
      coverImage { extraLarge large medium }
    } } }
    characters(sort: [ROLE, FAVOURITES_DESC], page: 1, perPage: 25) {
      edges { role node { id name { full native alternative } image { large medium } }
        voiceActors(language: JAPANESE) { id name { full native alternative } image { large medium } }
      }
    }
    staff(sort: [RELEVANCE, FAVOURITES_DESC], page: 1, perPage: 25) {
      edges { role node { id name { full native alternative } image { large medium } } }
    }
    airingSchedule(page: 1, perPage: 50) {
      nodes { id airingAt timeUntilAiring episode mediaId }
      pageInfo { currentPage lastPage hasNextPage total }
    }
    recommendations(sort: RATING_DESC, page: 1, perPage: 25) {
      nodes { rating mediaRecommendation {
        id idMal type format status(version: 2)
        title { romaji english native userPreferred }
        startDate { year month day }
        coverImage { extraLarge large medium }
      } }
    }
    stats { scoreDistribution { score amount } statusDistribution { status amount } }
  }
  byMal: Media(idMal: $id, type: ANIME) @include(if: $useMal) {
    id idMal type format status(version: 2)
    title { romaji english native userPreferred }
    description(asHtml: true)
    startDate { year month day }
    endDate { year month day }
    season seasonYear episodes duration genres synonyms
    weightedScore meanScore popularity
    coverImage { extraLarge large medium }
    bannerImage
    trailer { id site thumbnail }
    nextAiringEpisode { id airingAt timeUntilAiring episode mediaId }
    studios(isMain: true) { nodes { name } }
    tags { name }
    siteUrl
    relations { edges { relationType node {
      id idMal type format status(version: 2)
      title { romaji english native userPreferred }
      startDate { year month day }
      coverImage { extraLarge large medium }
    } } }
    characters(sort: [ROLE, FAVOURITES_DESC], page: 1, perPage: 25) {
      edges { role node { id name { full native alternative } image { large medium } }
        voiceActors(language: JAPANESE) { id name { full native alternative } image { large medium } }
      }
    }
    staff(sort: [RELEVANCE, FAVOURITES_DESC], page: 1, perPage: 25) {
      edges { role node { id name { full native alternative } image { large medium } } }
    }
    airingSchedule(page: 1, perPage: 50) {
      nodes { id airingAt timeUntilAiring episode mediaId }
      pageInfo { currentPage lastPage hasNextPage total }
    }
    recommendations(sort: RATING_DESC, page: 1, perPage: 25) {
      nodes { rating mediaRecommendation {
        id idMal type format status(version: 2)
        title { romaji english native userPreferred }
        startDate { year month day }
        coverImage { extraLarge large medium }
      } }
    }
    stats { scoreDistribution { score amount } statusDistribution { status amount } }
  }
}
"#;

#[derive(Debug, Deserialize)]
struct Envelope<T> { data: Option<T>, errors: Option<Vec<ApiError>> }
#[derive(Debug, Deserialize)] struct ApiError { message: String }
#[derive(Debug, Deserialize)] struct Page<T> { #[serde(rename = "Page")] page: PageData<T> }
#[derive(Debug, Deserialize)] struct PageData<T> { media: Vec<T> }
#[derive(Debug, Deserialize)] struct MediaResponse {
    #[serde(rename = "byId")] by_id: Option<MediaNode>,
    #[serde(rename = "byMal")] by_mal: Option<MediaNode>,
}
#[derive(Debug, Deserialize)] struct ShikimoriResponse { data: Option<ShikimoriData>, errors: Option<Vec<ApiError>> }
#[derive(Debug, Deserialize)] struct ShikimoriData { animes: Vec<ShikimoriNode> }
#[derive(Debug, Deserialize, Clone)] #[serde(untagged)] enum IdValue { Number(i64), Text(String) }
impl IdValue {
    fn as_string(&self) -> String { match self { Self::Number(value) => value.to_string(), Self::Text(value) => value.clone() } }
    fn as_i64(&self) -> Option<i64> { match self { Self::Number(value) => Some(*value), Self::Text(value) => value.parse().ok() } }
}
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")]
struct ShikimoriNode {
    id: String, mal_id: Option<IdValue>, name: String, russian: Option<String>, english: Option<String>, japanese: Option<String>,
    synonyms: Vec<String>, kind: Option<String>, status: Option<String>, aired_on: Option<FuzzyDate>, poster: Option<ShikimoriPoster>,
}
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct ShikimoriPoster { original_url: Option<String>, preview_url: Option<String> }

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(default, rename_all = "camelCase")]
struct MediaNode {
    id: i64, id_mal: Option<i64>, #[serde(rename = "type")] media_type: Option<String>, format: Option<String>, status: Option<String>,
    title: TitleNode, description: Option<String>, start_date: FuzzyDate, end_date: FuzzyDate,
    season_year: Option<i32>, episodes: Option<i32>, duration: Option<i32>, genres: Vec<String>, synonyms: Vec<String>,
    weighted_score: Option<i32>, mean_score: Option<i32>, popularity: Option<i32>, cover_image: Option<Cover>,
    banner_image: Option<String>, trailer: Option<Trailer>, next_airing_episode: Option<Airing>, studios: Option<Connection<Studio>>,
    tags: Vec<Tag>, site_url: Option<String>, relations: Option<RelationConnection>, characters: Option<CharacterConnection>,
    staff: Option<StaffConnection>, airing_schedule: Option<AiringConnection>, recommendations: Option<RecommendationConnection>,
    stats: Option<Stats>,
}
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct TitleNode { romaji: Option<String>, english: Option<String>, native: Option<String>, user_preferred: Option<String> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct FuzzyDate { year: Option<i32>, month: Option<i32>, day: Option<i32> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct Cover { extra_large: Option<String>, large: Option<String>, medium: Option<String> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct Trailer { id: Option<String>, site: Option<String>, thumbnail: Option<String> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct Airing { id: i64, airing_at: i64, time_until_airing: i64, episode: i32, media_id: i64 }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default)] struct Studio { name: String }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default)] struct Tag { name: String }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct Connection<T> { nodes: Vec<T> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct RelationConnection { edges: Vec<RelationEdge> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct RelationEdge { relation_type: String, node: Option<MediaNode> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct CharacterConnection { edges: Vec<CharacterEdge> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct CharacterEdge { role: Option<String>, node: Option<CharacterNode>, voice_actors: Vec<StaffNode> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct CharacterNode { id: i64, name: NameNode, image: Option<ImageNode> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct StaffConnection { edges: Vec<StaffEdge> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct StaffEdge { role: Option<String>, node: Option<StaffNode> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct StaffNode { id: i64, name: NameNode, image: Option<ImageNode> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct NameNode { full: Option<String>, native: Option<String>, alternative: Vec<String> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct ImageNode { large: Option<String>, medium: Option<String> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct AiringConnection { nodes: Vec<Airing>, page_info: PageInfo }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct PageInfo { current_page: u32, last_page: u32, has_next_page: bool, total: u32 }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct RecommendationConnection { nodes: Vec<Recommendation> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct Recommendation { rating: Option<i32>, media_recommendation: Option<MediaNode> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct Stats { score_distribution: Vec<Distribution>, status_distribution: Vec<StatusDistribution> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default)] struct Distribution { score: Option<i32>, amount: Option<i32> }
#[derive(Debug, Default, Deserialize, Clone)] #[serde(default, rename_all = "camelCase")] struct StatusDistribution { status: Option<String>, amount: Option<i32> }

fn gql<T: DeserializeOwned>(query: &str, variables: serde_json::Value) -> Result<T, String> {
    let response: Envelope<T> = anm_sdk::post_json(API, &serde_json::json!({ "query": query, "variables": variables }))?;
    if let Some(error) = response.errors.and_then(|mut errors| errors.drain(..).next()) { return Err(format!("AniList GraphQL: {}", error.message)); }
    response.data.ok_or_else(|| "AniList GraphQL: empty data".into())
}

const SHIKIMORI_BATCH_QUERY: &str = r#"
query AnimeBatch($ids: String!) {
  animes(ids: $ids, page: 1, limit: 50) {
    id malId name russian english japanese synonyms kind status
    airedOn { year month day }
    poster { originalUrl previewUrl }
  }
}
"#;

fn fetch_shikimori_cards(ids: &[i64]) -> HashMap<String, ShikimoriNode> {
    let mut unique: Vec<String> = ids.iter().map(ToString::to_string).collect();
    unique.sort();
    unique.dedup();
    let mut result = HashMap::new();
    for chunk in unique.chunks(50) {
        let body = serde_json::json!({ "query": SHIKIMORI_BATCH_QUERY, "variables": { "ids": chunk.join(",") } });
        let response: Result<ShikimoriResponse, String> = anm_sdk::post_json(SHIKIMORI_API, &body);
        let Ok(response) = response else { continue; };
        if response.errors.is_some() { continue; }
        for anime in response.data.map(|data| data.animes).unwrap_or_default() {
            result.insert(anime.id.clone(), anime.clone());
            if let Some(mal_id) = anime.mal_id.as_ref() { result.insert(mal_id.as_string(), anime); }
        }
    }
    result
}

fn shikimori_status(value: Option<&str>) -> AiringStatus {
    match value.unwrap_or_default() {
        "anons" => AiringStatus::Anons,
        "ongoing" => AiringStatus::Ongoing,
        "released" => AiringStatus::Released,
        _ => AiringStatus::Unknown,
    }
}

fn enrich_title_card(mut title: TitleCard, shikimori: Option<&ShikimoriNode>) -> TitleCard {
    let Some(shikimori) = shikimori else { return title; };
    let main = shikimori.russian.as_deref().filter(|value| !value.trim().is_empty()).unwrap_or(&shikimori.name).to_string();
    let mut alternatives = title.alternative_names;
    for (value, language) in [(shikimori.name.as_str(), "unknown"), (shikimori.english.as_deref().unwrap_or_default(), "en"), (shikimori.japanese.as_deref().unwrap_or_default(), "ja")] {
        if !value.trim().is_empty() && !value.eq_ignore_ascii_case(&main) && !alternatives.iter().any(|item| item.name.eq_ignore_ascii_case(value)) {
            alternatives.push(AlternativeName { name: value.to_string(), language: language.into() });
        }
    }
    for value in &shikimori.synonyms {
        if !value.trim().is_empty() && !value.eq_ignore_ascii_case(&main) && !alternatives.iter().any(|item| item.name.eq_ignore_ascii_case(value)) {
            alternatives.push(AlternativeName { name: value.clone(), language: "unknown".into() });
        }
    }
    if let Some(id) = shikimori.mal_id.as_ref().and_then(IdValue::as_i64).or_else(|| shikimori.id.parse().ok()) {
        if !title.tracker_data.iter().any(|link| link.name == "Shikimori") {
            title.tracker_data.push(TrackerData { name: "Shikimori".into(), data: id.to_string() });
        }
    }
    title.name = main;
    title.alternative_names = alternatives;
    title.poster = shikimori.poster.as_ref().and_then(|poster| poster.original_url.clone().or_else(|| poster.preview_url.clone())).or(title.poster);
    title.format = shikimori.kind.clone().map(|value| value.to_ascii_lowercase()).or(title.format);
    title.status = shikimori_status(shikimori.status.as_deref());
    title.year = shikimori.aired_on.as_ref().and_then(|date| date.year).and_then(|value| u16::try_from(value).ok()).or(title.year);
    title.url = Some(format!("https://shikimori.io/animes/{}", shikimori.id));
    title
}
enum LookupId { AniList(i64), Mal(i64) }
fn valid_id(raw: &str) -> Result<LookupId, String> {
    let value = raw.trim();
    let (kind, id) = value.split_once(':').map_or(("anilist", value), |(kind, id)| (kind, id));
    let id = id.parse().map_err(|_| format!("AniList: invalid anime id '{raw}'"))?;
    match kind.to_ascii_lowercase().as_str() {
        "anilist" => Ok(LookupId::AniList(id)),
        "mal" | "myanimelist" => Ok(LookupId::Mal(id)),
        _ => Err(format!("AniList: unsupported anime id '{raw}'")),
    }
}
fn nonempty(value: Option<String>) -> Option<String> { value.filter(|v| !v.trim().is_empty()) }
fn name(node: &NameNode) -> String { node.full.clone().or_else(|| node.native.clone()).unwrap_or_else(|| "Без имени".into()) }
fn alternatives(title: &TitleNode, main: &str, synonyms: &[String]) -> Vec<AlternativeName> {
    let mut values = Vec::new();
    for (value, language) in [(title.english.as_ref(), "en"), (title.native.as_ref(), "ja"), (title.romaji.as_ref(), "romaji")] {
        if let Some(value) = value { if !value.trim().is_empty() && !value.eq_ignore_ascii_case(main) && !values.iter().any(|a: &AlternativeName| a.name.eq_ignore_ascii_case(value)) { values.push(AlternativeName { name: value.clone(), language: language.into() }); } }
    }
    for value in synonyms { if !value.trim().is_empty() && !value.eq_ignore_ascii_case(main) && !values.iter().any(|a: &AlternativeName| a.name.eq_ignore_ascii_case(value)) { values.push(AlternativeName { name: value.clone(), language: "unknown".into() }); } }
    values
}
fn poster(media: &MediaNode) -> Option<String> { media.cover_image.as_ref().and_then(|c| c.extra_large.clone().or_else(|| c.large.clone()).or_else(|| c.medium.clone())) }
fn year(date: &FuzzyDate, fallback: Option<i32>) -> Option<u16> { date.year.or(fallback).and_then(|v| u16::try_from(v).ok()) }
fn status(value: Option<&str>) -> AiringStatus { match value.unwrap_or_default() { "RELEASING" => AiringStatus::Ongoing, "FINISHED" | "CANCELLED" => AiringStatus::Released, "NOT_YET_RELEASED" | "HIATUS" => AiringStatus::Anons, _ => AiringStatus::Unknown } }
fn format(value: Option<&str>) -> Option<String> { value.map(str::to_ascii_lowercase) }
fn tracker_data(media: &MediaNode) -> Vec<TrackerData> { let mut out = vec![TrackerData { name: "AniList".into(), data: media.id.to_string() }]; if let Some(id) = media.id_mal { out.push(TrackerData { name: "MyAnimeList".into(), data: id.to_string() }); } out }
fn preview(media: &MediaNode) -> AnimePreview {
    let title = nonempty(media.title.user_preferred.clone()).or_else(|| nonempty(media.title.romaji.clone())).or_else(|| nonempty(media.title.english.clone())).unwrap_or_else(|| format!("AniList #{}", media.id));
    AnimePreview { id: media.id.to_string(), title: title.clone(), alternative_names: alternatives(&media.title, &title, &media.synonyms), genres: media.genres.clone(), description: media.description.clone(), poster: poster(media), year: year(&media.start_date, media.season_year), score: media.weighted_score.map(|v| v as f32 / 10.0), kind: format(media.format.as_deref()), status: status(media.status.as_deref()), episodes_count: media.episodes.and_then(|v| u32::try_from(v).ok()), tracker_data: tracker_data(media), source_ref: None }
}
fn release(media: &MediaNode) -> Release {
    let p = preview(media); Release { id: media.id.to_string(), name: p.title.clone(), alternative_names: p.alternative_names.clone(), tracker_data: p.tracker_data.clone(), other_info: if media.tags.is_empty() { vec![] } else { vec![InfoBlock { title: "Теги".into(), data: media.tags.iter().map(|t| t.name.clone()).collect() }] }, banner_url: media.banner_image.clone(), poster: p.poster.clone(), description: media.description.clone(), genres: media.genres.clone(), studios: media.studios.as_ref().map(|s| s.nodes.iter().map(|v| v.name.clone()).collect()).unwrap_or_default(), status: p.status, kind: p.kind.clone(), year: p.year, trailer: media.trailer.as_ref().and_then(|t| trailer_url(t)), total_episodes: media.episodes.and_then(|v| u32::try_from(v).ok()), next_episode_at: media.next_airing_episode.as_ref().and_then(|a| chrono::DateTime::from_timestamp(a.airing_at, 0)).map(|d| d.to_rfc3339()), rating: media.weighted_score.map(|v| v as f32 / 10.0), source_ref: None }
}
fn trailer_url(t: &Trailer) -> Option<String> { let id = t.id.as_deref()?; match t.site.as_deref().unwrap_or_default().to_ascii_lowercase().as_str() { "youtube" => Some(format!("https://www.youtube.com/watch?v={id}")), "dailymotion" => Some(format!("https://www.dailymotion.com/video/{id}")), _ => Some(format!("https://{}/{}", t.site.as_deref().unwrap_or("anilist.co"), id)) } }
fn image(image: Option<&ImageNode>) -> Option<String> { image.and_then(|v| v.large.clone().or_else(|| v.medium.clone())) }
fn character_data(id: i64) -> Vec<TrackerData> { vec![TrackerData { name: "AniList".into(), data: id.to_string() }] }
fn character_record(edge: CharacterEdge) -> Option<exports::animex::module::tracker_character_roles_api::CharacterWithRole> { let node = edge.node?; let character = Character { id: node.id.to_string(), name: name(&node.name), alternative_names: node.name.alternative.iter().map(|v| AlternativeName { name: v.clone(), language: "unknown".into() }).collect(), tracker_data: character_data(node.id), image: image(node.image.as_ref()), seiyuu: edge.voice_actors.into_iter().map(staff_ref).collect() }; Some(exports::animex::module::tracker_character_roles_api::CharacterWithRole { character, role: edge.role.map(|v| match v.as_str() { "MAIN" => "main", "SUPPORTING" => "supporting", _ => "none" }.into()) }) }
fn staff_ref(node: StaffNode) -> StaffRef { StaffRef { id: node.id.to_string(), name: name(&node.name), alternative_names: node.name.alternative.iter().map(|v| AlternativeName { name: v.clone(), language: "unknown".into() }).collect(), image: image(node.image.as_ref()) } }
fn staff_member(edge: StaffEdge) -> Option<StaffMember> { let node = edge.node?; Some(StaffMember { id: node.id.to_string(), name: name(&node.name), alternative_names: node.name.alternative.iter().map(|v| AlternativeName { name: v.clone(), language: "unknown".into() }).collect(), role: edge.role.unwrap_or_else(|| "Staff".into()), image: image(node.image.as_ref()) }) }
fn card(media: MediaNode) -> TitleCard { let p = preview(&media); TitleCard { id: media.id.to_string(), name: p.title, alternative_names: p.alternative_names, poster: p.poster, media_type: if media.media_type.as_deref() == Some("MANGA") { MediaType::Manga } else { MediaType::Anime }, format: format(media.format.as_deref()), status: status(media.status.as_deref()), year: year(&media.start_date, media.season_year), tracker_data: tracker_data(&media), source_ref: None, url: media.site_url.or_else(|| Some(format!("{SITE}/anime/{}", media.id))) } }
fn episode(item: Airing) -> TrackerEpisode { TrackerEpisode { id: item.id.to_string(), number: item.episode as f64, title: None, alternative_names: vec![], aired_at: chrono::DateTime::from_timestamp(item.airing_at, 0).map(|v| v.to_rfc3339()), rating: None, score_scale: 10, is_filler: None, is_recap: None, url: Some(format!("{SITE}/anime/{}/{}", item.media_id, item.episode)), forum_url: None, video_url: None } }
fn statistics(media: &MediaNode) -> TitleStatistics { let scores = media.stats.as_ref().map(|s| s.score_distribution.iter().filter_map(|v| Some(ScoreCount { score: u8::try_from(v.score?).ok()?, count: u32::try_from(v.amount?).ok()? })).collect()).unwrap_or_default(); let statuses = media.stats.as_ref().map(|s| s.status_distribution.iter().filter_map(|v| { let raw = v.status.clone()?; let count = u32::try_from(v.amount?).ok()?; Some(StatusCount { label: raw.to_ascii_lowercase().replace('_', " "), status: raw, count }) }).collect()).unwrap_or_default(); TitleStatistics { rating: media.weighted_score.map(|v| v as f32 / 10.0), score_scale: 10, scores, statuses } }

struct Component;
impl Guest for Component {
    fn search(query: String, limit: u32) -> Result<Vec<AnimePreview>, String> { let data: Page<MediaNode> = gql(SEARCH_QUERY, serde_json::json!({ "search": query, "perPage": limit.clamp(1, 50) }))?; Ok(data.page.media.iter().map(preview).collect()) }
    fn get_anime(id: String) -> Result<Release, String> { let media = fetch(&id)?; Ok(release(&media)) }
    fn get_characters(id: String) -> Result<Vec<Character>, String> { Ok(fetch(&id)?.characters.map(|c| c.edges.into_iter().filter_map(character_record).map(|v| v.character).collect()).unwrap_or_default()) }
    fn get_staff(id: String) -> Result<Vec<StaffMember>, String> { Ok(fetch(&id)?.staff.map(|s| s.edges.into_iter().filter_map(staff_member).collect()).unwrap_or_default()) }
    fn get_videos(id: String) -> Result<Vec<TrackerVideo>, String> { let media = fetch(&id)?; Ok(media.trailer.into_iter().filter_map(|t| Some(TrackerVideo { id: t.id.clone()?, kind: VideoKind::Trailer, name: Some("AniList trailer".into()), url: trailer_url(&t)?, thumbnail: t.thumbnail })).collect()) }
    fn get_screenshots(_id: String) -> Result<Vec<Screenshot>, String> { Ok(vec![]) }
}
fn fetch(raw: &str) -> Result<MediaNode, String> {
    let lookup = valid_id(raw)?;
    let id = match lookup { LookupId::AniList(id) | LookupId::Mal(id) => id };
    let is_mal = matches!(lookup, LookupId::Mal(_));
    let response = gql::<MediaResponse>(DETAILS_QUERY, serde_json::json!({
        "id": id,
        "useAniList": !is_mal,
        "useMal": is_mal,
    }))?;
    match lookup {
        LookupId::AniList(_) => response.by_id,
        LookupId::Mal(_) => response.by_mal,
    }.ok_or_else(|| format!("AniList: anime '{raw}' not found"))
}

impl exports::animex::module::tracker_character_roles_api::Guest for Component { fn get_characters(id: String) -> Result<Vec<exports::animex::module::tracker_character_roles_api::CharacterWithRole>, String> { Ok(fetch(&id)?.characters.map(|c| c.edges.into_iter().filter_map(character_record).collect()).unwrap_or_default()) } }
impl exports::animex::module::related_titles_api::Guest for Component {
    fn get_related(release: ReleaseRef) -> Result<Vec<RelatedTitle>, String> {
        let entries: Vec<(TitleCard, Option<i64>, TitleRelation)> = fetch(&release.id)?.relations
            .map(|relations| relations.edges.into_iter().filter_map(|edge| {
                let node = edge.node?;
                let mal_id = node.id_mal;
                Some((card(node), mal_id, TitleRelation { code: edge.relation_type.clone(), label: edge.relation_type.to_ascii_lowercase().replace('_', " ") }))
            }).collect()).unwrap_or_default();
        let shikimori = fetch_shikimori_cards(&entries.iter().filter_map(|(_, id, _)| *id).collect::<Vec<_>>());
        Ok(entries.into_iter().flat_map(|(title, mal_id, relation)| {
            let enriched = mal_id.and_then(|id| shikimori.get(&id.to_string())).map(|item| RelatedTitle { title: enrich_title_card(title.clone(), Some(item)), relation: relation.clone() });
            std::iter::once(RelatedTitle { title, relation: relation.clone() }).chain(enriched).collect::<Vec<_>>()
        }).collect())
    }
}
impl exports::animex::module::similar_titles_api::Guest for Component {
    fn get_similar(release: ReleaseRef) -> Result<Vec<SimilarTitle>, String> {
        let entries: Vec<(TitleCard, Option<i64>, Option<i32>)> = fetch(&release.id)?.recommendations
            .map(|recommendations| recommendations.nodes.into_iter().filter_map(|recommendation| {
                let node = recommendation.media_recommendation?;
                let mal_id = node.id_mal;
                Some((card(node), mal_id, recommendation.rating))
            }).collect()).unwrap_or_default();
        let shikimori = fetch_shikimori_cards(&entries.iter().filter_map(|(_, id, _)| *id).collect::<Vec<_>>());
        Ok(entries.into_iter().flat_map(|(title, mal_id, rating)| {
            let reason = rating.map(|value| format!("{} рекомендаций", value));
            let enriched = mal_id.and_then(|id| shikimori.get(&id.to_string())).map(|item| SimilarTitle { title: enrich_title_card(title.clone(), Some(item)), reason: reason.clone() });
            std::iter::once(SimilarTitle { title, reason: reason.clone() }).chain(enriched).collect::<Vec<_>>()
        }).collect())
    }
}
impl exports::animex::module::statistics_api::Guest for Component { fn get_statistics(release: ReleaseRef) -> Result<TitleStatistics, String> { Ok(statistics(&fetch(&release.id)?)) } }
impl exports::animex::module::tracker_episodes_api::Guest for Component { fn get_episodes(id: String, page: u32) -> Result<TrackerEpisodePage, String> { let media = fetch(&id)?; let schedule = media.airing_schedule.ok_or_else(|| "AniList: airing schedule unavailable".to_string())?; Ok(TrackerEpisodePage { items: schedule.nodes.into_iter().map(episode).collect(), meta: animex::module::types::PageMeta { page: page.max(1), per_page: 50, total: Some(schedule.page_info.total), total_pages: Some(schedule.page_info.last_page), has_next: schedule.page_info.has_next_page } }) } }

export!(Component);
