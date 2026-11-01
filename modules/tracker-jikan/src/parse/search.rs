//! MyAnimeList anime search results parser.
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/Parser/Search/AnimeSearchParser.php` — result table, last page and
//!   `hasNextPage` extraction.
//! - `src/Parser/Search/AnimeSearchListItemParser.php` — title, poster,
//!   synopsis, type, episodes, score, members, rating and air dates.
//! - `src/Request/Search/AnimeSearchRequest.php` — `/anime.php?q=...&cat=...`
//!   query construction and the `show` offset.
//! - `src/Helper/Parser.php` — `parseDateMDY` / airing heuristics.
//!
//! MAL removed the members/start/end/rated columns from the default search
//! result table in 2026. The parser therefore reads the first five columns
//! always, and uses columns 6–9 only when the older nine-column layout is
//! served. Missing values stay `None`/`Unknown`; nothing is inferred from a
//! neighbouring column.

use crate::animex::module::types::{AiringStatus, AnimePreview};
use dom_query::{Document, NodeRef, Selection};
use std::collections::HashSet;
use url::Url;

use crate::html::{clean_text, image_url, node_text};
use crate::model::{
    anime_kind, episode_count, id_from_href, score_from_text, tracker_links, year_from_text,
    EntryRef, SITE,
};

/// Build the MAL search path. `cat=anime` keeps manga results out. The module's
/// `limit` is intentionally not sent to MAL: it caps only the local result list
/// (same rule as the previous jikan-edge integration).
pub fn path(query: &str) -> Result<String, String> {
    let query = query.trim();
    if query.chars().count() > 64 {
        return Err("MyAnimeList: поисковый запрос не должен превышать 64 символа".into());
    }
    let mut url = Url::parse(SITE).map_err(|error| error.to_string())?;
    url.set_path("/anime.php");
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("cat", "anime");
    Ok(format!(
        "{}?{}",
        url.path(),
        url.query().unwrap_or_default()
    ))
}

pub fn parse(document: &Document, limit: u32) -> Vec<AnimePreview> {
    let limit = limit.min(50) as usize;
    if limit == 0 {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for row in document
        .select("div.js-categories-seasonal table tr")
        .nodes()
        .iter()
        .cloned()
    {
        let selection = Selection::from(row);
        let cells = selection.select("td");
        if cells.length() < 5 {
            continue;
        }
        let Some(link) = find_title_link(&selection) else {
            continue;
        };
        let Some(href) = link.attr("href").map(|value| value.to_string()) else {
            continue;
        };
        let Some(id) = id_from_href(&href, "anime") else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let entry = EntryRef { id, manga: false };
        let name = node_text(&link)
            .map(|name| name.trim_end_matches(" add").trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("#{id}"));

        let image = selection
            .select("td:nth-child(1) img")
            .nodes()
            .first()
            .and_then(image_url);
        let description = selection
            .select("td:nth-child(2) div.pt4")
            .nodes()
            .first()
            .map(|node| clean_text(&node.text()))
            .map(|text| {
                text.trim_end_matches("read more.")
                    .trim_end_matches("read more")
                    .trim()
                    .to_string()
            })
            .filter(|text| !text.is_empty());

        let type_name = cell_text(&cells, 2);
        let episodes = episode_count(cell_text(&cells, 3).as_deref());
        let score = cell_text(&cells, 4).as_deref().and_then(score_from_text);
        let start = cell_text(&cells, 5);
        let end = cell_text(&cells, 6);
        let status = if cells.length() >= 9 {
            status_from_dates(episodes, start.as_deref(), end.as_deref())
        } else {
            AiringStatus::Unknown
        };
        let year = start.as_deref().and_then(year_from_text);

        result.push(AnimePreview {
            id: entry.key(),
            title: name,
            alternative_names: Vec::new(),
            genres: Vec::new(),
            description,
            poster: image,
            year,
            score,
            kind: anime_kind(type_name.as_deref(), false),
            status,
            episodes_count: episodes,
            tracker_data: tracker_links(&entry.key()),
            source_ref: None,
        });
        if result.len() >= limit {
            break;
        }
    }
    result
}

fn find_title_link<'a>(selection: &Selection<'a>) -> Option<NodeRef<'a>> {
    let direct = selection
        .select("td:nth-child(2) div.title > a")
        .nodes()
        .iter()
        .cloned()
        .find(|link| {
            link.attr("href")
                .and_then(|href| id_from_href(&href.to_string(), "anime"))
                .is_some()
                && !clean_text(&link.text()).eq_ignore_ascii_case("add")
        });
    direct.or_else(|| {
        selection
            .select("td:nth-child(2) a strong")
            .nodes()
            .first()
            .and_then(|strong| strong.parent())
    })
}

fn cell_text(cells: &Selection<'_>, index: usize) -> Option<String> {
    let cell = cells.get(index)?;
    let text = clean_text(&cell.text());
    (!text.is_empty() && text != "-").then_some(text)
}

fn status_from_dates(
    episodes: Option<u32>,
    start: Option<&str>,
    end: Option<&str>,
) -> AiringStatus {
    // Jikan: an entry with one episode cannot be airing.
    if episodes == Some(1) {
        return AiringStatus::Unknown;
    }
    let Some(start) = start.and_then(parse_date) else {
        return AiringStatus::Unknown;
    };
    let now = current_ymd();
    if start > now {
        return AiringStatus::Anons;
    }
    match end.and_then(parse_date) {
        None => AiringStatus::Ongoing,
        Some(end) if end < now => AiringStatus::Released,
        Some(_) => AiringStatus::Ongoing,
    }
}

/// Parse `Oct 3, 2002`, `Oct 2002` or `2002`. `?` / `??` fields stay unknown.
fn parse_date(raw: &str) -> Option<(i32, u32, u32)> {
    let raw = clean_text(raw);
    if raw.is_empty() || raw == "-" || raw.contains('?') {
        return None;
    }
    let tokens: Vec<&str> = raw
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let mut year = None;
    let mut month = None;
    let mut day = None;
    for token in tokens {
        if token.len() == 4 && token.bytes().all(|byte| byte.is_ascii_digit()) {
            year = token.parse::<i32>().ok();
        } else if let Some(index) = months
            .iter()
            .position(|name| name.eq_ignore_ascii_case(token))
        {
            month = Some(index as u32 + 1);
        } else if token.len() <= 2 && token.bytes().all(|byte| byte.is_ascii_digit()) {
            day = token.parse::<u32>().ok();
        }
    }
    Some((year?, month.unwrap_or(1), day.unwrap_or(1).clamp(1, 31)))
}

fn current_ymd() -> (i32, u32, u32) {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    civil_from_days(seconds.div_euclid(86_400))
}

/// Howard Hinnant's `civil_from_days`, avoiding a chrono dependency.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let leap = if m <= 2 { 1 } else { 0 };
    ((y + leap) as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::document;

    #[test]
    fn query_is_encoded_and_limited_without_sending_limit_to_mal() {
        let built = path("  ナルト & Bleach?limit=999  ").unwrap();
        let url = Url::parse(&format!("{SITE}{built}")).unwrap();
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("q".into(), "ナルト & Bleach?limit=999".into()));
        assert_eq!(pairs[1], ("cat".into(), "anime".into()));
        assert!(path(&"я".repeat(65)).is_err());
        assert!(path(&"я".repeat(64)).is_ok());
    }

    #[test]
    fn current_five_column_results_are_parsed() {
        let document = document(include_str!("../../tests/fixtures/search.html"));
        let results = parse(&document, 10);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].id, "20");
        assert_eq!(results[0].title, "Naruto");
        assert_eq!(results[0].kind.as_deref(), Some("tv"));
        assert_eq!(results[0].episodes_count, Some(220));
        assert_eq!(results[0].score, Some(8.03));
        assert_eq!(
            results[0].poster.as_deref(),
            Some("https://cdn.myanimelist.net/images/anime/1141/142503.jpg")
        );
        assert!(results[0].description.as_deref().unwrap().contains("ninja"));
        assert_eq!(results[0].tracker_data[0].name, "MyAnimeList");
        assert_eq!(parse(&document, 2).len(), 2);
    }

    #[test]
    fn header_and_add_links_are_not_treated_as_titles() {
        let document = document(include_str!("../../tests/fixtures/search.html"));
        let results = parse(&document, 50);
        assert!(!results
            .iter()
            .any(|item| item.title.eq_ignore_ascii_case("add")));
        assert!(results.iter().all(|item| item.id != "0"));
    }

    #[test]
    fn old_nine_column_layout_supplies_dates_and_status() {
        let document = document(
            r#"<html><body><div class="js-categories-seasonal"><table>
            <tr><td>h</td><td>Title</td><td>Type</td><td>Eps.</td><td>Score</td></tr>
            <tr>
              <td><img data-src="https://cdn.myanimelist.net/r/50x70/images/anime/1/20.jpg"></td>
              <td><div class="title"><a href="/anime/20/Naruto"><strong>Naruto</strong></a></div><div class="pt4">Ninja story. <a>read more.</a></div></td>
              <td>TV</td><td>220</td><td>8.03</td><td>Oct 3, 2002</td><td>Feb 8, 2007</td><td>3,173,820</td><td>PG-13</td>
            </tr>
            </table></div></body></html>"#,
        );
        let results = parse(&document, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].year, Some(2002));
        assert!(matches!(results[0].status, AiringStatus::Released));
    }

    #[test]
    fn missing_dates_do_not_invent_statuses() {
        assert_eq!(
            status_from_dates(Some(12), None, None),
            AiringStatus::Unknown
        );
        assert_eq!(
            status_from_dates(Some(1), Some("Jan 1, 2000"), Some("Dec 31, 2001")),
            AiringStatus::Unknown
        );
        assert_eq!(
            status_from_dates(Some(12), Some("-"), Some("-")),
            AiringStatus::Unknown
        );
        assert_eq!(
            status_from_dates(Some(12), Some("Jan 1, 2000"), Some("Dec 31, 2001")),
            AiringStatus::Released
        );
    }
}
