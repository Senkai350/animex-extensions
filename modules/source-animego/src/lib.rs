//! AnimeGO source module (`org.animego.source`).
//!
//! Implements the WIT `source-api` interface on top of the animego.me HTML
//! site. Endpoints and card markup were verified against live site captures
//! and cross-checked with three community parsers (anime-parsers-ru,
//! anime-dl-core, moe-parsers):
//!
//! - catalog:  `GET /anime?page=N&sort=…` (card grid `div.ani-grid__item`)
//! - top:      `GET /anime/top-250?page=N`
//! - search:   `GET /search/anime?q=…`
//! - release:  `GET /anime/{slug}-{id}` (`div.entity` markup)
//! - episodes: `GET /player/{animegoId}` + `/player/{animegoId}/episodes?page=N`
//! - players:  `GET /player/{animegoId}?episode={episodePage}` (XHR JSON)
//!
//! All network access goes through `anm_sdk`, so the host enforces the
//! domain allowlist. Successful metadata caching belongs to the host.

wit_bindgen::generate!({
    path: "../../vendor/sdk/wit",
    world: "source-module-release-details",
});

use anm_sdk::protocol::provider_embed::{CvhContext, EmbedPayload, EmbedVariant};

use animex::module::types::{
    AiringStatus, AlternativeName, AnimePreview, CatalogPage, CatalogParams, Episode, EpisodeRef,
    FilterField, FilterSelect, InfoBlock, OptionPair, PageMeta, PlayerCandidate, LegacyRelease as Release,
    ReleaseRef, TrackerData,
};
use exports::animex::module::catalog_sorting_api::Guest as SortingGuest;
use exports::animex::module::filter_ui_api::Guest as FilterUiGuest;
use exports::animex::module::source_api::Guest;
use serde::Deserialize;
use url::Url;

const DEFAULT_MIRROR: &str = "https://animego.me";

// ---------------------------------------------------------------------------
// Host HTTP helpers (no independent metadata cache).
// ---------------------------------------------------------------------------

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";

fn base() -> String {
    let mirror = anm_sdk::config_get("mirror")
        .ok()
        .flatten()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    mirror.unwrap_or_else(|| DEFAULT_MIRROR.to_string())
}

fn site_headers(xhr: bool) -> Vec<(String, String)> {
    let mut headers = vec![
        ("user-agent".to_string(), USER_AGENT.to_string()),
        ("referer".to_string(), format!("{}/", base())),
    ];
    if xhr {
        headers.push(("x-requested-with".to_string(), "XMLHttpRequest".to_string()));
    }
    headers
}

/// The host owns successful metadata caching, expiry and forced refresh.
/// Playback pages and embed URLs are never persisted in a guest data cache.
fn fetch(_key: &str, url: &str, xhr: bool) -> Result<String, String> {
    let response = anm_sdk::http_request(&anm_sdk::HttpRequest {
        url: url.to_string(),
        method: None,
        headers: site_headers(xhr),
        body: None,
    })?;
    if !(200..300).contains(&response.status) {
        return Err(format!("http status {} for {url}", response.status));
    }
    Ok(String::from_utf8_lossy(&response.body).to_string())
}

fn fetch_content(url: &str) -> Result<String, String> {
    let body = fetch(url, url, true)?;
    parse_xhr(&body)
        .and_then(|parsed| parsed.content().map(str::to_string))
        .ok_or_else(|| "animego: invalid player response".to_string())
}

// ---------------------------------------------------------------------------
// Minimal HTML helpers (dom_query, byte-level pre-strip like tracker-jikan).
// ---------------------------------------------------------------------------

pub(crate) fn document(body: &str) -> dom_query::Document {
    dom_query::Document::from(strip_heavy_nodes(body).as_str())
}

fn find_ci(haystack: &str, needle: &str, start: usize) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (start..=haystack.len() - needle.len())
        .find(|&index| haystack[index..index + needle.len()].eq_ignore_ascii_case(needle))
}

fn strip_heavy_nodes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let script = find_ci(input, "<script", cursor);
        let style = find_ci(input, "<style", cursor);
        let next = [script, style].into_iter().flatten().min();
        let Some(start) = next else {
            output.push_str(&input[cursor..]);
            break;
        };
        output.push_str(&input[cursor..start]);
        let (opening, closing) = if style == Some(start) {
            ("<style", "</style")
        } else {
            ("<script", "</script")
        };
        let after_open = start + opening.len();
        let end = find_ci(input, closing, after_open)
            .and_then(|end| input[end..].find('>').map(|offset| end + offset + 1))
            .unwrap_or(input.len());
        // Keep structured release metadata; executable scripts and styles are discarded.
        let opening_end = input[after_open..]
            .find('>')
            .map(|offset| after_open + offset)
            .unwrap_or(after_open);
        if opening == "<script"
            && input[start..opening_end]
                .to_ascii_lowercase()
                .contains("application/ld+json")
        {
            output.push_str(&input[start..end]);
        }
        cursor = end;
    }
    output
}

/// Collapse whitespace and NBSP variants, drop control characters.
pub(crate) fn clean_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut pending_space = false;
    for character in input.chars() {
        let character = match character {
            '\u{00a0}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}' => ' ',
            other => other,
        };
        if character.is_whitespace() {
            pending_space = !output.is_empty();
            continue;
        }
        if character.is_control() {
            continue;
        }
        if pending_space {
            output.push(' ');
            pending_space = false;
        }
        output.push(character);
    }
    output.trim().to_string()
}

/// Decode HTML entities that appear inside JSON payloads and attributes.
pub(crate) fn unescape_html(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    input
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

// ---------------------------------------------------------------------------
// XHR JSON envelope (`{"content": ...}` or `{"data": {"content": ...}}`).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct XhrContent {
    content: Option<String>,
    data: Option<ContentData>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct ContentData {
    content: Option<String>,
}

impl XhrContent {
    fn content(&self) -> Option<&str> {
        if let Some(content) = self.content.as_deref() {
            if !content.is_empty() {
                return Some(content);
            }
        }
        self.data
            .as_ref()
            .and_then(|data| data.content.as_deref())
            .filter(|content| !content.is_empty())
    }
}

pub(crate) fn parse_xhr(body: &str) -> Option<XhrContent> {
    serde_json::from_str(body).ok()
}

// ---------------------------------------------------------------------------
// Player buttons and episode pages in the player HTML.
// ---------------------------------------------------------------------------

/// One AnimeGO voice button in the player HTML.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct VoiceButton {
    pub player: String,
    pub label: String,
    pub embed: String,
    pub translation_id: Option<String>,
}

/// Exact provider labels AnimeGO puts into `data-provider-title`.
pub(crate) const PLAYER_ANIBOOM: &str = "AniBoom";
pub(crate) const PLAYER_CVH: &str = "CVH";
pub(crate) const PLAYER_KODIK: &str = "Kodik";

/// Parse `VoiceButton`s out of the player HTML fragment.
///
/// Buttons carry `data-player` (embed), `data-translation-title` (dub) and
/// `data-provider-title` (player kind). The ` (ошибка)` suffix marks broken
/// players and is skipped.
pub(crate) fn parse_voice_buttons(html: &str) -> Vec<VoiceButton> {
    let document = document(html);
    let mut buttons = Vec::new();
    for node in document.select("button[data-player]").nodes() {
        let node = dom_query::Selection::from(*node);
        let Some(embed) = node
            .attr("data-player")
            .map(|value| value.trim().to_string())
        else {
            continue;
        };
        if embed.is_empty() {
            continue;
        }
        let label = node
            .attr("data-translation-title")
            .map(|value| clean_text(&unescape_html(&value)))
            .unwrap_or_default();
        if label.is_empty() {
            continue;
        }
        if label.to_lowercase().contains("(ошибка)") {
            continue;
        }
        let embed = if embed.starts_with("//") {
            format!("https:{embed}")
        } else {
            embed
        };
        let player = node
            .attr("data-provider-title")
            .map(|value| clean_text(&unescape_html(&value)))
            .unwrap_or_else(|| "unknown".to_string());
        let translation_id = node
            .attr("data-ptranslation")
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty());
        buttons.push(VoiceButton {
            player,
            label,
            embed,
            translation_id,
        });
    }
    buttons
}

/// `(episode_number, data_episode_page)` pairs in the player HTML.
/// The anime ID for later episodes is the `episode` player page number.
pub(crate) fn parse_episode_pages(html: &str) -> Vec<(u32, String)> {
    let document = document(html);
    let mut out = Vec::new();
    for node in document.select("[data-episode]").nodes() {
        let node = dom_query::Selection::from(*node);
        let Some(number) = node
            .attr("data-episode-number")
            .and_then(|value| value.trim().parse::<u32>().ok())
        else {
            continue;
        };
        let Some(page) = node
            .attr("data-episode")
            .map(|value| value.trim().to_string())
        else {
            continue;
        };
        if !page.is_empty() {
            out.push((number, page));
        }
    }
    out
}

pub(crate) fn episode_page_id(html: &str, number: u32) -> Option<String> {
    parse_episode_pages(html)
        .into_iter()
        .find(|(found, _)| *found == number)
        .map(|(_, page)| page)
}

// ---------------------------------------------------------------------------
// Card grid parsers (catalog/search/top share the same item markup).
// ---------------------------------------------------------------------------

/// Stable AnimeGO id of a card: trailing number of `/anime/{slug}-{id}`.
pub(crate) fn id_from_href(href: &str) -> Option<String> {
    let last = href.trim_end_matches('/').rsplit('/').next()?;
    let index = last.rfind('-')?;
    let slug = &last[..index];
    // Navigation pseudo-pages (/anime/top-250, /anime/random) share the shape.
    if matches!(
        slug.to_ascii_lowercase().as_str(),
        "top" | "random" | "ongoing" | "type"
    ) {
        return None;
    }
    let digits = &last[index + 1..];
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then(|| digits.to_string())
}

/// One card of the AnimeGO item grid (live homepage capture).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CardItem {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub alt_title: Option<String>,
    pub poster: Option<String>,
    pub score: Option<f32>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub year: Option<u16>,
    pub kind: Option<String>,
}

/// Parse all cards from an AnimeGO listing page.
///
/// Two card markups are known: the home/season grid (`ani-grid__item`, home
/// page capture) and the older search/catalog grid (`animes-grid-item`,
/// verified in the moe-parsers/anime-parsers-ru sources). Selectors accept
/// both so a markup mix never breaks the listing.
pub(crate) fn parse_cards(html: &str) -> Vec<CardItem> {
    let document = document(html);
    let mut cards = Vec::new();
    let mut seen = Vec::new();
    for node in document
        .select("div.ani-grid__item, div.ani-list__item, div.animes-grid-item")
        .nodes()
    {
        let card = dom_query::Selection::from(*node);
        let link = card
            .select("a.ani-grid__item-picture[href], a.d-block[href*='/anime/']")
            .nodes()
            .iter()
            .find_map(|a| a.attr("href").map(|value| value.to_string()))
            .or_else(|| {
                card.select("a[href*='/anime/']")
                    .nodes()
                    .iter()
                    .find_map(|a| a.attr("href").map(|value| value.to_string()))
            });
        let Some(link) = link else { continue };
        let Some(id) = id_from_href(&link) else {
            continue;
        };
        if seen.iter().any(|known: &String| known == &id) {
            continue;
        }
        let without_id = link.trim_start_matches("/anime/").trim_end_matches('/');
        let slug = match without_id.rfind('-') {
            Some(index) => without_id[..index].to_string(),
            None => without_id.to_string(),
        };
        let title = card
            .select("a[title][href*='/anime/'], a.d-block[title]")
            .nodes()
            .iter()
            .find_map(|a| {
                a.attr("title")
                    .map(|value| clean_text(&unescape_html(&value)))
            })
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let alt_title = card
            .select("div.text-line-clamp, div.text-gray-dark-6")
            .nodes()
            .iter()
            .find_map(|div| {
                let text = clean_text(&div.text());
                (!text.is_empty()).then_some(text)
            });
        let poster = card
            .select("img.image__img, img.anime-grid-lazy")
            .nodes()
            .iter()
            .find_map(|img| {
                img.attr("src")
                    .or_else(|| img.attr("data-original"))
                    .map(|value| value.to_string())
            })
            .filter(|value| !value.is_empty());
        let score = card
            .select("div.rating-badge")
            .nodes()
            .iter()
            .find_map(|badge| {
                let text = clean_text(&badge.text());
                text.replace(',', ".").trim().parse::<f32>().ok()
            });
        let description = card
            .select(".ani-list__item-description")
            .nodes()
            .iter()
            .find_map(|node| {
                let text = clean_text(&node.text());
                (!text.is_empty()).then_some(text)
            });
        let genres = card
            .select(".ani-list__item-genres a[href*='/anime/genre/']")
            .nodes()
            .iter()
            .map(|node| clean_text(&node.text()))
            .filter(|text| !text.is_empty())
            .collect();
        let year = card
            .select(
                "a[href*='/anime/season/'], .ani-grid__item-genres .ani-grid__item-genres__link",
            )
            .nodes()
            .iter()
            .find_map(|node| {
                node.attr("href")
                    .and_then(|href| year_from_text(href.as_ref()))
                    .or_else(|| year_from_text(&node.text()))
            });
        let kind = card
            .select("a[href*='/anime/type/'], .ani-grid__item-genres .ani-grid__item-genres__link")
            .nodes()
            .iter()
            .find_map(|node| {
                let href = node.attr("href");
                let slug = href
                    .as_ref()
                    .filter(|href| href.contains("/anime/type/"))
                    .and_then(|href| href.trim_end_matches('/').rsplit('/').next());
                let label = match slug {
                    Some("tv") | Some("movie") | Some("ova") | Some("ona") | Some("special") => {
                        slug
                    }
                    _ => kind_from_label(&node.text()),
                };
                label.map(str::to_string)
            });
        seen.push(id.clone());
        cards.push(CardItem {
            id,
            slug,
            title,
            alt_title,
            poster,
            score,
            description,
            genres,
            year,
            kind,
        });
    }
    cards
}

/// Whether the listing page has a next page: the pagination block carries a
/// `next` control or the current page is not the last in `link-square` style.
pub(crate) fn has_next_page(html: &str, page: u32) -> bool {
    let document = document(html);
    if let Some(value) = document
        .select("[data-infinite-scroll-has-more-value]")
        .attr("data-infinite-scroll-has-more-value")
    {
        return value.as_ref() == "true";
    }
    // a[rel="next"] is the standard marker; keep a numeric fallback.
    if document.select("a[rel=next]").exists() {
        return true;
    }
    // `ul.pagination li a` listing `/anime?page=N+1` also proves more pages.
    for link in document.select("ul.pagination a").nodes() {
        let Some(href) = link.attr("href") else {
            continue;
        };
        let marker = format!("page={}", page + 1);
        if href.contains(&marker) {
            return true;
        }
    }
    false
}

pub(crate) fn preview_from_card(card: &CardItem) -> AnimePreview {
    let mut alternative_names = Vec::new();
    if let Some(alt) = card.alt_title.as_deref() {
        if !alt.eq_ignore_ascii_case(&card.title) {
            alternative_names.push(AlternativeName {
                name: alt.to_string(),
                language: "unknown".to_string(),
            });
        }
    }
    AnimePreview {
        id: format!("{}-{}", card.slug, card.id),
        title: card.title.clone(),
        alternative_names,
        genres: card.genres.clone(),
        description: card.description.clone(),
        poster: card.poster.clone(),
        year: card.year,
        score: card.score,
        kind: card.kind.clone(),
        status: AiringStatus::Unknown,
        episodes_count: None,
        tracker_data: Vec::new(),
        source_ref: None,
    }
}

/// Source ref that ties episodes/players back to the AnimeGO numeric id.
pub(crate) fn source_ref(id: &str, slug: &str) -> Option<String> {
    let value = serde_json::json!({ "animegoId": id, "slug": slug });
    Some(value.to_string())
}

// ---------------------------------------------------------------------------
// Release page parsers.
// ---------------------------------------------------------------------------

/// Known `div.entity-field` labels, from the verified anime-parsers-ru
/// mapping (k == label, v+1 == value). Communities track the exact list.
const ENTITY_LABELS: &[&str] = &[
    "Тип",
    "Эпизоды",
    "Жанры",
    "Тема",
    "Первоисточник",
    "Сезон",
    "Статус",
    "Выпуск",
    "Рейтинг",
    "Возраст",
    "Длительность",
    "Студия",
    "Озвучка",
    "Автор",
    "Режиссёр",
    "Главные герои",
    "Следующий эпизод",
];

fn is_known_label(text: &str) -> bool {
    ENTITY_LABELS
        .iter()
        .any(|label| text.eq_ignore_ascii_case(label))
}

/// Label/value rows of the `div.entity-field` block.
pub(crate) fn entity_fields(document: &dom_query::Document) -> Vec<(String, Vec<String>)> {
    let mut fields = Vec::new();
    let container = document.select("div.entity-field");
    if !container.exists() {
        return fields;
    }
    let rows = document.select("div.entity-field > div");
    let mut pending: Option<String> = None;
    for row in rows.nodes() {
        let row = dom_query::Selection::from(*row);
        let text = clean_text(&row.text());
        if text.is_empty() {
            continue;
        }
        if is_known_label(&text) {
            pending = Some(text);
            continue;
        }
        let Some(label) = pending.take() else {
            continue;
        };
        let mut values = Vec::new();
        for link in row.select("a").nodes() {
            let link_text = clean_text(&link.text());
            if !link_text.is_empty() && values.iter().all(|known| known != &link_text) {
                values.push(link_text);
            }
        }
        if values.is_empty() {
            values.push(text.clone());
        }
        fields.push((label, values));
    }
    fields
}

/// `span.entity-rating__value` score of the release page.
fn rating_from_text(value: &str) -> Option<f32> {
    let text = clean_text(value).replace(',', ".");
    text.trim().parse::<f32>().ok().filter(|score| *score > 0.0)
}

/// Map the AnimeGO status label to the canonical airing status.
pub(crate) fn airing_status_from_label(label: &str) -> AiringStatus {
    match clean_text(label).to_lowercase().as_str() {
        label if label.contains("онгоинг") => AiringStatus::Ongoing,
        label if label.contains("вышел") || label.contains("анонс нет") => {
            AiringStatus::Released
        }
        label if label.contains("анонс") => AiringStatus::Anons,
        _ => AiringStatus::Unknown,
    }
}

/// Map the AnimeGO type label to the canonical kind.
pub(crate) fn kind_from_label(label: &str) -> Option<&'static str> {
    match clean_text(label).to_lowercase().as_str() {
        label if label.contains("тв") || label == "сериал" => Some("tv"),
        label if label.contains("фильм") => Some("movie"),
        label if label.contains("ova") => Some("ova"),
        label if label.contains("ona") => Some("ona"),
        label if label.contains("спешл") => Some("special"),
        _ => None,
    }
}

/// Find a four digit year inside free text.
pub(crate) fn year_from_text(value: &str) -> Option<u16> {
    let bytes = value.as_bytes();
    let mut start = 0;
    while start + 4 <= bytes.len() {
        if bytes[start..start + 4].iter().all(u8::is_ascii_digit) {
            let year: u16 = value[start..start + 4].parse().ok()?;
            if (1900..=2100).contains(&year) {
                return Some(year);
            }
        }
        start += 1;
    }
    None
}

/// Episode count from `Эпизоды` label value: `12 / 24` → 12, `12` → 12.
pub(crate) fn episodes_count_from_label(value: &str) -> Option<u32> {
    let text = clean_text(value);
    let head = text.split('/').next().unwrap_or("");
    let digits: String = head.chars().filter(|c| c.is_ascii_digit()).collect();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

// ---------------------------------------------------------------------------
// Episode schedule parser.
// ---------------------------------------------------------------------------

/// One row of the `episodeSchedule` table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScheduleRow {
    pub number: u32,
    pub title: Option<String>,
    pub released: bool,
}

/// Parse `div.row.m-0` schedule rows (numbers come from `meta[content]`).
pub(crate) fn parse_schedule(html: &str) -> Vec<ScheduleRow> {
    let document = document(html);
    let mut rows = Vec::new();
    for node in document.select("div.row.m-0").nodes() {
        let node = dom_query::Selection::from(*node);
        let Some(number) = node
            .select("meta[content]")
            .nodes()
            .iter()
            .find_map(|meta| {
                meta.attr("content")
                    .and_then(|value| value.trim().parse::<u32>().ok())
            })
        else {
            continue;
        };
        let cells = node.select("div");
        let nodes = cells.nodes();
        let title = nodes
            .get(1)
            .map(|cell| clean_text(&cell.text()))
            .filter(|text| !text.is_empty() && text != "---");
        let released = node.select("span[data-watched-id]").exists();
        rows.push(ScheduleRow {
            number,
            title,
            released,
        });
    }
    rows
}

// ---------------------------------------------------------------------------
// Guest implementation.
// ---------------------------------------------------------------------------

struct Component;

impl FilterUiGuest for Component {
    fn get_filter_ui(
    ) -> Result<Vec<exports::animex::module::filter_ui_api::FilterPresentation>, String> {
        use exports::animex::module::filter_ui_api::{ChoicePresentation, FilterPresentation};
        Ok(vec![
            FilterPresentation {
                id: "type".into(),
                presentation: ChoicePresentation::RadioSelect,
            },
            FilterPresentation {
                id: "sort".into(),
                presentation: ChoicePresentation::Dropdown,
            },
        ])
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
            directions: Vec::new(),
            default_direction: None,
        }))
    }
}

fn sort_options() -> Vec<OptionPair> {
    labeled_options(&[
        ("RATE", "По рейтингу"),
        ("UPDATED", "По дате добавления"),
        ("NEW", "Сначала новые"),
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
        Ok(vec![FilterField::Select(FilterSelect {
            id: "type".to_string(),
            label: "Тип".to_string(),
            options: labeled_options(&[
                ("tv", "ТВ-сериал"),
                ("movie", "Фильм"),
                ("ova", "OVA"),
                ("ona", "ONA"),
                ("special", "Спешл"),
            ]),
            default: None,
        })])
    }

    fn get_catalog(params: CatalogParams) -> Result<CatalogPage, String> {
        if let Some(query) = params
            .search
            .clone()
            .filter(|value| !value.trim().is_empty())
        {
            return Self::search(query, params);
        }
        fetch_listing(
            &build_list_url(
                params.page,
                "anime",
                params.search.as_deref(),
                &params.filters,
                &sort_of(&params),
            ),
            params.page,
        )
    }

    fn get_latest(page: u32) -> Result<CatalogPage, String> {
        fetch_listing(&build_list_url(page, "anime", None, &[], "NEW"), page)
    }

    fn get_popular(page: u32) -> Result<CatalogPage, String> {
        fetch_listing(&build_list_url(page, "anime", None, &[], "RATE"), page)
    }

    fn search(query: String, params: CatalogParams) -> Result<CatalogPage, String> {
        fetch_listing(
            &build_list_url(
                params.page,
                "search/anime",
                Some(&query),
                &params.filters,
                "RATE",
            ),
            params.page,
        )
    }

    fn get_release(release: ReleaseRef) -> Result<Release, String> {
        if release.id.trim().is_empty() {
            return Err("release id must not be empty".into());
        }
        let key = format!("animego:release:{}", release.id);
        let url = release_url(&release.id)?;
        let body = fetch(&key, &url, false)?;
        map_release(&body, &release.id)
    }

    fn get_episodes(release: ReleaseRef) -> Result<Vec<Episode>, String> {
        let id = numeric_release_id(&release.id)?;
        let duration = release
            .payload
            .as_deref()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
            .and_then(|value| value["episodeDurationSeconds"].as_u64())
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0);
        let root = format!("{}/player/{id}", base());
        let content = fetch_content(&root)?;
        let mut pages = parse_episode_pages(&content);
        let doc = document(&content);
        let page_count = doc
            .select("[data-anime-player-episodes-pages-value]")
            .attr("data-anime-player-episodes-pages-value")
            .and_then(|value| serde_json::from_str::<Vec<serde_json::Value>>(&value).ok())
            .map(|pages| pages.len())
            .unwrap_or(1);
        if page_count > 100 {
            return Err("animego: too many episode pages".into());
        }
        for page in 1..page_count {
            pages.extend(parse_episode_pages(&fetch_content(&format!(
                "{root}/episodes?page={page}"
            ))?));
        }
        pages.sort_by_key(|(number, _)| *number);
        pages.dedup_by_key(|(number, _)| *number);
        Ok(pages
            .into_iter()
            .map(|(number, page)| Episode {
                id: format!("{}-{number}", release.id),
                number,
                title: None,
                thumbnail: None,
                duration: duration.map(|value| value as f32),
                time_skips: Vec::new(),
                source_ref: Some(serde_json::json!({"episodePage": page}).to_string()),
            })
            .collect())
    }

    fn get_players(
        release: ReleaseRef,
        episode: EpisodeRef,
    ) -> Result<Vec<PlayerCandidate>, String> {
        let id = numeric_release_id(&release.id)?;
        let number = episode
            .id
            .rsplit('-')
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|number| *number > 0)
            .ok_or("animego: invalid episode number")?;
        let root = format!("{}/player/{id}", base());
        let page = episode
            .payload
            .as_deref()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
            .and_then(|value| {
                value
                    .get("episodePage")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        let page = match page {
            Some(page) => page,
            None => {
                let content = fetch_content(&root)?;
                let found = episode_page_id(&content, number);
                match found {
                    Some(page) => page,
                    None => {
                        // Episode menus are paged in blocks of 100 by the site.
                        let content =
                            fetch_content(&format!("{root}/episodes?page={}", (number - 1) / 100))?;
                        episode_page_id(&content, number).ok_or("animego: episode not found")?
                    }
                }
            }
        };
        if page.is_empty() || !page.bytes().all(|c| c.is_ascii_digit()) {
            return Err("animego: invalid episode page".into());
        }
        let content = fetch_content(&format!("{root}?episode={page}"))?;
        Ok(player_candidates(&content, &id, number))
    }
}

anm_sdk::release_metadata!();

impl exports::animex::module::release_details_api::Guest for Component {
    fn get_release_details(reference: ReleaseRef) -> Result<crate::animex::module::types::Release, String> {

        let url = release_url(&reference.id)?;
        let body = fetch(&format!("animego:release:{}", reference.id), &url, false)?;
        let mut details = release_metadata::upgrade(map_release(&body, &reference.id)?);
        let document = document(&body);
        let structured = structured_release(&document);
        let field = |prefix: &str| details.other_info.iter()
            .find(|block| block.title.to_lowercase().starts_with(prefix))
            .and_then(|block| block.data.first()).map(String::as_str);
        details.age_rating = field("возраст").or_else(|| field("рейтинг mpaa")).and_then(release_metadata::age_rating);
        details.original_source = field("первоисточник").or_else(|| field("источник")).and_then(release_metadata::original_source);
        details.episode_duration_seconds = field("длительность").and_then(release_metadata::duration_text)
            .or_else(|| details.source_ref.as_deref().and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                .and_then(|value| value["episodeDurationSeconds"].as_u64()).and_then(|seconds| u32::try_from(seconds).ok()));
        details.aired_from = structured["datePublished"].as_str().and_then(release_metadata::iso_date);
        details.aired_to = structured["endDate"].as_str().and_then(release_metadata::iso_date);
        details.score_scale = details.rating.map(|_| 10);
        details.url = structured["url"].as_str().filter(|url| url.starts_with("https://")).map(str::to_owned).or(Some(url));
        for genre in &mut details.genres {
            genre.language = Some("ru".into());
            genre.url = document.select("div.entity a[href*='/genre/'], div.entity a[href*='/genres/']")
                .nodes().iter().find(|link| clean_text(&link.text()) == genre.name)
                .and_then(|link| link.attr("href").map(|href| href.to_string()))
                .filter(|href| href.starts_with('/') && !href.starts_with("//"))
                .map(|href| format!("{}{href}", base().trim_end_matches('/')));
        }
        Ok(details)
    }
}

export!(Component);

// ---------------------------------------------------------------------------
// Request building.
// ---------------------------------------------------------------------------

fn build_list_url(
    page: u32,
    path: &str,
    search: Option<&str>,
    filters: &[(String, String)],
    sort: &str,
) -> String {
    build_list_url_for(&base(), page, path, search, filters, sort)
}

fn build_list_url_for(
    base: &str,
    page: u32,
    path: &str,
    search: Option<&str>,
    filters: &[(String, String)],
    sort: &str,
) -> String {
    let mut path = path.trim_start_matches('/').to_string();
    if !path.contains("search") {
        if let Some((_, kind)) = filters
            .iter()
            .find(|(key, value)| key == "type" && !value.trim().is_empty())
        {
            path = format!("anime/type/{}", option_value(kind));
        }
        if page > 1 {
            path = format!("{path}/{page}");
        }
    }
    match Url::parse(&format!("{base}/{path}")) {
        Ok(mut url) => {
            let mut query = url.query_pairs_mut();
            if path.contains("search") {
                if let Some(term) = search.map(str::trim).filter(|value| !value.is_empty()) {
                    query.append_pair("q", term);
                }
                if page > 1 {
                    query.append_pair("page", &page.to_string());
                }
            } else if !path.contains("top-250") {
                let sort = match sort {
                    "UPDATED" => "createdAt",
                    "NEW" => "startDate",
                    _ => "rating",
                };
                query
                    .append_pair("sort", sort)
                    .append_pair("direction", "desc");
                query.append_pair("view", "list");
            }
            drop(query);
            url.to_string().trim_end_matches('?').to_string()
        }
        Err(_) => format!("{base}/{path}"),
    }
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

fn sort_of(params: &CatalogParams) -> String {
    params
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
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "RATE".into())
}

fn release_url(id: &str) -> Result<String, String> {
    release_url_for(&base(), id)
}

fn release_url_for(base: &str, id: &str) -> Result<String, String> {
    Url::parse(&format!("{base}/anime/{id}"))
        .map(|url| url.to_string())
        .map_err(|e| format!("release url: {e}"))
}

fn fetch_listing(url: &str, page: u32) -> Result<CatalogPage, String> {
    let key = format!("animego:list:{url}");
    let body = fetch(&key, url, false)?;
    let cards = parse_cards(&body);
    let items: Vec<AnimePreview> = cards.iter().map(preview_from_card).collect();
    let total = items.len() as u32;
    let has_next = has_next_page(&body, page);
    Ok(CatalogPage {
        items,
        meta: PageMeta {
            page,
            // AnimeGO does not advertise a per-page size, leave it to the host.
            per_page: total.max(1),
            total: None,
            total_pages: None,
            has_next,
        },
    })
}

// ---------------------------------------------------------------------------
// Release mapping.
// ---------------------------------------------------------------------------

/// Read only title-level JSON-LD, ignoring unrelated website/breadcrumb entries.
fn structured_release(document: &dom_query::Document) -> serde_json::Value {
    fn find(value: &serde_json::Value) -> Option<serde_json::Value> {
        if let Some(items) = value.as_array() {
            return items.iter().find_map(find);
        }
        let kind = &value["@type"];
        if kind
            .as_str()
            .is_some_and(|kind| matches!(kind, "TVSeries" | "Movie" | "TVSeason"))
            || kind.as_array().is_some_and(|items| {
                items.iter().any(|kind| {
                    kind.as_str()
                        .is_some_and(|kind| matches!(kind, "TVSeries" | "Movie" | "TVSeason"))
                })
            })
        {
            return Some(value.clone());
        }
        value.get("@graph").and_then(find)
    }
    document
        .select("script[type='application/ld+json']")
        .nodes()
        .iter()
        .filter_map(|node| serde_json::from_str::<serde_json::Value>(&node.text()).ok())
        .find_map(|value| find(&value))
        .unwrap_or(serde_json::Value::Null)
}

fn release_tracker_data(document: &dom_query::Document) -> Vec<TrackerData> {
    let base = Url::parse(DEFAULT_MIRROR).expect("valid AnimeGO base URL");
    let mut result: Vec<TrackerData> = Vec::new();
    for node in document.select(".content-ref-ids a[href]").nodes().iter() {
        let Some(href) = node.attr("href") else { continue };
        let Ok(url) = base.join(href.trim()) else { continue };
        if !matches!(url.scheme(), "http" | "https") { continue; }
        let host = url.host_str().unwrap_or_default().trim_start_matches("www.");
        let path: Vec<_> = url.path().trim_matches('/').split('/').collect();
        let (name, id) = match host {
            "shikimori.io" | "shikimori.one" | "shikimori.me" | "shikimori.org"
                if path.first() == Some(&"animes") => ("Shikimori", path.get(1).copied().map(str::to_string)),
            "myanimelist.net" if path.first() == Some(&"anime") =>
                ("MyAnimeList", path.get(1).copied().map(str::to_string)),
            "world-art.ru" if url.path() == "/animation/animation.php" =>
                ("WorldArt", url.query_pairs().find(|(key, _)| key == "id").map(|(_, id)| id.into_owned())),
            "kinopoisk.ru" if matches!(path.first(), Some(&"film" | &"series")) =>
                ("Kinopoisk", path.get(1).copied().map(str::to_string)),
            _ => continue,
        };
        let Some(id) = id else { continue };
        // Shikimori links may include a title slug after the numeric identifier.
        let id = if name == "Shikimori" { id.split('-').next().unwrap_or_default() } else { &id };
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) { continue; }
        let Ok(id) = id.parse::<u64>() else { continue };
        if id == 0 || result.iter().any(|item| item.name == name) { continue; }
        result.push(TrackerData { name: name.into(), data: id.to_string() });
    }
    result
}

fn map_release(body: &str, requested_id: &str) -> Result<Release, String> {
    let document = document(body);
    let entity = document.select("div.entity");
    if !entity.exists() {
        return Err(format!(
            "animego: release page {requested_id} has no entity block"
        ));
    }

    let structured = structured_release(&document);
    let title = document
        .select("div.entity h1, h1")
        .nodes()
        .iter()
        .map(|node| clean_text(&node.text().to_string()))
        .find(|text| !text.is_empty())
        .unwrap_or_default();

    let mut alt_titles: Vec<AlternativeName> = document
        .select("div.entity__title-synonyms")
        .nodes()
        .iter()
        .find_map(|node| Some(clean_text(&node.text())))
        .map(|text| {
            text.split('/')
                .map(clean_text)
                .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(&title))
                .map(|name| {
                    let language = if name
                        .chars()
                        .any(|c| (c as u32) >= 0x3040 && (c as u32) <= 0x30FF)
                    {
                        "ja"
                    } else if name
                        .chars()
                        .all(|c| (c as u32) < 0x0400 || (c as u32) > 0x04FF)
                    {
                        "unknown"
                    } else {
                        "ru"
                    };
                    AlternativeName {
                        name,
                        language: language.to_string(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    if alt_titles.is_empty() {
        let aliases = &structured["alternateName"];
        let aliases = aliases
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![aliases.clone()]);
        alt_titles = aliases
            .iter()
            .filter_map(|value| value.as_str())
            .map(clean_text)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(&title))
            .map(|name| AlternativeName {
                name,
                language: "unknown".into(),
            })
            .collect();
    }

    let poster = document
        .select("div.entity__poster img.image__img, div.entity img.image__img")
        .nodes()
        .iter()
        .find_map(|img| img.attr("src").map(|value| value.to_string()))
        .filter(|value| !value.is_empty())
        .or_else(|| structured["image"].as_str().map(str::to_string));
    let banner = poster.clone();

    let description = document
        .select("div.description")
        .nodes()
        .iter()
        .find_map(|node| {
            let text = clean_text(&node.text());
            (!text.is_empty() && text.len() > 40).then_some(text)
        })
        .or_else(|| structured["description"].as_str().map(str::to_string));

    let fields = entity_fields(&document);
    let mut other_info = Vec::new();
    for (label, values) in &fields {
        other_info.push(InfoBlock {
            title: label.clone(),
            data: values.clone(),
        });
    }

    let field_value = |name: &str| -> Option<String> {
        fields
            .iter()
            .find(|(label, _)| clean_text(label).to_lowercase().contains(name))
            .and_then(|(_, values)| values.first().cloned())
    };

    let genres: Vec<String> = fields
        .iter()
        .find(|(label, _)| label.eq_ignore_ascii_case("Жанры"))
        .map(|(_, values)| values.clone())
        .unwrap_or_default();

    let rating = document
        .select("span.entity-rating__value")
        .nodes()
        .iter()
        .find_map(|node| rating_from_text(&node.text()))
        .or_else(|| {
            structured["aggregateRating"]["ratingValue"]
                .as_f64()
                .map(|value| value as f32)
        });

    let status = field_value("статус")
        .as_deref()
        .map(airing_status_from_label)
        .unwrap_or(AiringStatus::Unknown);
    let kind = field_value("тип").and_then(|value| kind_from_label(&value).map(str::to_string));
    let year = field_value("выпуск")
        .and_then(|value| year_from_text(&value))
        .or_else(|| {
            structured["datePublished"]
                .as_str()
                .and_then(year_from_text)
        });
    let total_episodes = field_value("эпизоды")
        .and_then(|value| episodes_count_from_label(&value))
        .or_else(|| {
            structured["numberOfEpisodes"]
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
        });

    Ok(Release {
        id: requested_id.to_string(),
        name: title,
        alternative_names: alt_titles,
        tracker_data: release_tracker_data(&document),
        other_info,
        banner_url: banner,
        poster,
        description,
        genres,
        studios: field_value("студия")
            .map(|value| vec![value])
            .unwrap_or_default(),
        status,
        kind,
        year,
        trailer: None,
        total_episodes,
        next_episode_at: None,
        rating,
        source_ref: source_ref_text(
            requested_id,
            field_value("длительность").and_then(|value| {
                let minutes = value.split_whitespace().next()?.parse::<u32>().ok()?;
                value
                    .split_whitespace()
                    .nth(1)?
                    .starts_with("мин")
                    .then(|| minutes.checked_mul(60))
                    .flatten()
            }),
        ),
    })
}

fn source_ref_text(id: &str, duration: Option<u32>) -> Option<String> {
    Some(
        serde_json::json!({ "animegoId": numeric_release_id(id).unwrap_or_else(|_| id.into()), "episodeDurationSeconds": duration })
            .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Player candidates.
// ---------------------------------------------------------------------------

fn numeric_release_id(id: &str) -> Result<String, String> {
    let value = id.rsplit('-').next().unwrap_or_default();
    if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
        return Err("animego: invalid release id".into());
    }
    Ok(value.to_string())
}

fn player_candidates(html: &str, release_id: &str, number: u32) -> Vec<PlayerCandidate> {
    player_candidates_for(&base(), html, release_id, number)
}

fn cvh_context(embed: &str, number: u32) -> Option<CvhContext> {
    let url = Url::parse(embed).ok()?;
    let parts: Vec<_> = url.path_segments()?.collect();
    if parts.first().copied() != Some("cdn-iframe") {
        return None;
    }
    let (voice, season, episode) = match parts.as_slice() {
        [_, _, voice, season, episode] => {
            let voice =
                url::form_urlencoded::parse(format!("v={}", voice.replace('+', "%2B")).as_bytes())
                    .find(|(key, _)| key == "v")?
                    .1
                    .into_owned();
            (voice, *season, *episode)
        }
        [_, _, season, episode] => {
            let voice = url
                .query_pairs()
                .find(|(key, _)| key == "dubbing")?
                .1
                .into_owned();
            (voice, *season, *episode)
        }
        _ => return None,
    };
    if voice.trim().is_empty() {
        return None;
    }

    Some(CvhContext {
        media_id: parts[1].into(),
        publisher_id: "747".into(),
        aggregator: "mali".into(),
        voice,
        season: season.parse().ok()?,
        episode: episode.parse().unwrap_or(number),
    })
}

fn player_candidates_for(
    site: &str,
    html: &str,
    _release_id: &str,
    number: u32,
) -> Vec<PlayerCandidate> {
    let buttons = parse_voice_buttons(html);
    [
        (PLAYER_CVH, "cvh.embed.v1"),
        (PLAYER_KODIK, "kodik.embed.v1"),
        (PLAYER_ANIBOOM, "aniboom.embed.v1"),
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(priority, (provider, protocol))| {
        let variants: Vec<_> = buttons
            .iter()
            .filter(|button| button.player.eq_ignore_ascii_case(provider))
            .enumerate()
            .map(|(index, button)| EmbedVariant {
                id: format!(
                    "{}-{index}",
                    button.translation_id.as_deref().unwrap_or("dub")
                ),
                label: button.label.clone(),
                url: button.embed.clone(),
                language: Some("ru".into()),
                cvh: if provider == "CVH" {
                    cvh_context(&button.embed, number)
                } else {
                    None
                },
            })
            .collect();
        if variants.is_empty() {
            return None;
        }
        Some(PlayerCandidate {
            protocol: protocol.into(),
            label: Some(provider.into()),
            priority: Some(priority as u32),
            payload: serde_json::to_string(&EmbedPayload {
                variants,
                referer: Some(format!("{site}/")),
            })
            .ok()?,
        })
    })
    .collect()
}

#[cfg(test)]
mod tests;
