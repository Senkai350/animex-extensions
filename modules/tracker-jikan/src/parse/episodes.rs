//! MAL episode metadata, adapted from jikan-me/jikan (MIT):
//! src/Parser/Anime/{EpisodesParser,EpisodeListItemParser}.php and
//! src/Request/Anime/AnimeEpisodesRequest.php. One request per 100-item page;
//! no per-episode detail requests and no automatic pagination.

use crate::animex::module::types::{AlternativeName, PageMeta, TrackerEpisode, TrackerEpisodePage};
use crate::html::clean_text;
use crate::model::{absolute_url, EntryRef, SITE};
use dom_query::{Document, Selection};
use std::collections::HashSet;

const PER_PAGE: u32 = 100;

pub fn request(entry: &EntryRef, page: u32) -> Result<(String, String), String> {
    let offset = page
        .checked_sub(1)
        .and_then(|page| page.checked_mul(PER_PAGE))
        .ok_or_else(|| "MyAnimeList: invalid episode page".to_string())?;
    Ok((
        format!("{}:offset:{offset}", entry.page_key(Some("episode"))),
        format!("{}?offset={offset}", entry.path(Some("episode"))),
    ))
}

pub fn empty_page(page: u32) -> TrackerEpisodePage {
    TrackerEpisodePage {
        items: vec![],
        meta: PageMeta {
            page,
            per_page: PER_PAGE,
            total: None,
            total_pages: None,
            has_next: false,
        },
    }
}

fn text(row: &Selection<'_>, selector: &str) -> Option<String> {
    let value = clean_text(&row.select_single(selector).text());
    (!value.is_empty() && !matches!(value.as_str(), "N/A" | "Unknown" | "-" | "?")).then_some(value)
}

fn link(row: &Selection<'_>, selector: &str) -> Option<String> {
    row.select_single(selector)
        .attr("href")
        .and_then(|href| absolute_url(&href))
}

/// MAL gives a calendar date, not a time. Preserve it and use the requested
/// midnight fallback without inventing a UTC offset (which could shift the day).
fn aired_at(raw: &str) -> Option<String> {
    let normalized = raw.replace(',', " ");
    let parts: Vec<_> = normalized.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|month| parts[0].eq_ignore_ascii_case(month))?
        + 1;
    let day = parts[1].parse::<u32>().ok()?;
    let year = parts[2]
        .parse::<u32>()
        .ok()
        .filter(|year| (1..=9999).contains(year))?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (day > 0 && day <= days).then(|| format!("{year:04}-{month:02}-{day:02}T00:00:00"))
}

fn alternative_names(row: &Selection<'_>) -> Vec<AlternativeName> {
    let Some(value) = text(row, "td.episode-title span.di-ib") else {
        return vec![];
    };
    let mut names = Vec::new();
    if let Some((romanji, japanese)) = value
        .rsplit_once('(')
        .filter(|(_, rest)| rest.ends_with(')'))
    {
        for (name, language) in [(romanji, "ja-Latn"), (japanese.trim_end_matches(')'), "ja")] {
            let name = clean_text(name);
            if !name.is_empty() {
                names.push(AlternativeName {
                    name,
                    language: language.into(),
                });
            }
        }
    } else {
        // Do not label an unstructured Japanese-only string as a romanization.
        let language = if value
            .chars()
            .any(|c| ('\u{3040}'..='\u{9fff}').contains(&c))
        {
            "ja"
        } else {
            "ja-Latn"
        };
        names.push(AlternativeName {
            name: value,
            language: language.into(),
        });
    }
    names
}

pub fn parse(
    document: &Document,
    entry: &EntryRef,
    page: u32,
) -> Result<TrackerEpisodePage, String> {
    request(entry, page)?;
    let mut result = empty_page(page);
    let table = document.select("table.js-watch-episode-list");
    let no_episodes = document
        .select("#content")
        .text()
        .contains("No episode information has been added to this title");
    if !table.exists() {
        if no_episodes {
            return Ok(result);
        }
        return Err("MyAnimeList: episode list not found (unexpected page layout)".into());
    }
    let mut seen = HashSet::new();
    for row in table.select("tr").iter() {
        let Some(id) = text(&row, "td.episode-number") else {
            continue;
        };
        let number = id
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0)
            .ok_or_else(|| "MyAnimeList: invalid episode number".to_string())?;
        if !seen.insert(id.clone()) {
            continue;
        }
        if !row.select("td.episode-title").exists() {
            return Err("MyAnimeList: episode title cell not found".into());
        }
        let markers = clean_text(
            &row.select("td.episode-title span.icon-episode-type-bg")
                .text(),
        );
        let rating = row
            .select_single("td.episode-poll")
            .attr("data-raw")
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value > 0.0 && *value <= 5.0);
        result.items.push(TrackerEpisode {
            id,
            number,
            title: text(&row, "td.episode-title > a"),
            alternative_names: alternative_names(&row),
            aired_at: text(&row, "td.episode-aired").and_then(|date| aired_at(&date)),
            rating,
            score_scale: 5,
            is_filler: Some(markers.contains("Filler")),
            is_recap: Some(markers.contains("Recap")),
            url: link(&row, "td.episode-title > a"),
            forum_url: link(&row, "td.episode-forum > a"),
            video_url: link(&row, "td.episode-video > a"),
        });
    }
    if result.items.is_empty() {
        return Ok(result);
    }
    // Restrict links to this anime's episode list. Forum/video/detail links
    // and pagination of other sections cannot create phantom episode pages.
    let mut last_page = page;
    for href in document
        .select("a[href*='offset=']")
        .nodes()
        .iter()
        .filter_map(|node| node.attr("href"))
    {
        let Ok(url) = url::Url::options()
            .base_url(Some(&url::Url::parse(SITE).unwrap()))
            .parse(&href)
        else {
            continue;
        };
        let path: Vec<_> = url.path().trim_matches('/').split('/').collect();
        if url.host_str() != Some("myanimelist.net")
            || path.len() != 4
            || path[0] != "anime"
            || path[1] != entry.id.to_string()
            || path[3] != "episode"
        {
            continue;
        }
        if let Some(offset) = url
            .query_pairs()
            .find(|(key, _)| key == "offset")
            .and_then(|(_, value)| value.parse::<u32>().ok())
            .filter(|offset| offset % PER_PAGE == 0)
        {
            last_page = last_page.max(offset / PER_PAGE + 1);
        }
    }
    result.meta.total_pages = Some(last_page);
    result.meta.has_next = last_page > page;
    // MAL does not supply an exact total in its list pagination. A short
    // fixture/page is not evidence of a smaller catalogue; leave total unknown.
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::document;
    fn entry() -> EntryRef {
        EntryRef::parse("20").unwrap()
    }

    #[test]
    fn parses_all_metadata_and_pagination() {
        let result = parse(
            &document(include_str!("../../tests/fixtures/episodes.html")),
            &entry(),
            1,
        )
        .unwrap();
        assert_eq!(result.items.len(), 3);
        let first = &result.items[0];
        assert_eq!(first.id, "1");
        assert_eq!(first.number, 1.0);
        assert_eq!(first.title.as_deref(), Some("Enter: Naruto Uzumaki!"));
        assert_eq!(first.alternative_names[0].language, "ja-Latn");
        assert_eq!(first.alternative_names[1].name, "参上! うずまきナルト");
        assert_eq!(first.aired_at.as_deref(), Some("2002-10-03T00:00:00"));
        assert_eq!(first.rating, Some(4.11));
        assert_eq!(first.score_scale, 5);
        assert_eq!(first.is_filler, Some(false));
        assert_eq!(first.is_recap, Some(false));
        assert_eq!(
            first.url.as_deref(),
            Some("https://myanimelist.net/anime/20/Naruto/episode/1")
        );
        assert!(first.forum_url.as_deref().unwrap().contains("topicid="));
        assert!(first.video_url.is_some());
        assert_eq!(result.items[1].is_filler, Some(true));
        assert_eq!(result.items[1].is_recap, Some(true));
        assert!(result.items[1].aired_at.is_none());
        assert!(result.items[1].rating.is_none());
        assert!(result.items[2].title.is_none());
        assert!(result.items[2].rating.is_none());
        assert!(result.items[2].video_url.is_none());
        assert_eq!(result.meta.total_pages, Some(3));
        assert!(result.meta.has_next);
        assert!(result.meta.total.is_none());
    }

    #[test]
    fn pages_have_distinct_cache_keys_and_checked_offsets() {
        let first = request(&entry(), 1).unwrap();
        let second = request(&entry(), 2).unwrap();
        assert_eq!(first.1, "/anime/20/_/episode?offset=0");
        assert_eq!(second.1, "/anime/20/_/episode?offset=100");
        assert_ne!(first.0, second.0);
        assert!(request(&entry(), 0).is_err());
        assert!(request(&entry(), u32::MAX).is_err());
    }

    #[test]
    fn empty_is_distinct_from_unexpected_html() {
        let empty =
            document("<div id='content'>No episode information has been added to this title</div>");
        assert!(parse(&empty, &entry(), 1).unwrap().items.is_empty());
        assert!(parse(
            &document("<div id='content'>Please log in</div>"),
            &entry(),
            1
        )
        .is_err());
        let last = document(include_str!("../../tests/fixtures/episodes-last.html"));
        let result = parse(&last, &entry(), 3).unwrap();
        assert_eq!(result.items[0].number, 201.0);
        assert!(!result.meta.has_next);
        assert_eq!(result.meta.page, 3);
    }

    #[test]
    fn validates_dates_and_uses_midnight_without_timezone_conversion() {
        assert_eq!(
            aired_at("Feb 29, 2024").as_deref(),
            Some("2024-02-29T00:00:00")
        );
        for value in [
            "Feb 29, 2023",
            "Apr 31, 2024",
            "Jan 0, 2024",
            "N/A",
            "Unknown",
            "Oct 2002",
        ] {
            assert!(aired_at(value).is_none(), "{value}");
        }
    }
}
