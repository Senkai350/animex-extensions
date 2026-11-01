//! Community recommendations ("similar titles").
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/Parser/Common/Recommendations.php` — each recommendation is a
//!   `div.borderClass` block.
//! - `src/Parser/Common/Recommendation.php` — title link, poster and
//!   `Read recommendations by <strong>N</strong> more users` vote count.
//! - `src/Request/Anime/AnimeRecommendationsRequest.php`,
//!   `MangaRecommendationsRequest.php` — `/anime/<id>/_/userrecs` pages.
//! - `src/Parser/Recommendations/*` — the inline carousel on the detail page.
//!
//! MAL's `N more users` count excludes the first recommendation, so Jikan adds
//! one. We keep that rule and sort by the total number of recommenders.

use crate::animex::module::types::{AiringStatus, MediaType, SimilarTitle, TitleCard};
use dom_query::{Document, NodeRef, Selection};
use std::collections::HashSet;

use crate::html::{clean_text, image_url, node_text};
use crate::model::{id_from_href, tracker_links};

pub fn recommendations(document: &Document, manga: bool, limit: u32) -> Vec<SimilarTitle> {
    let limit = limit.min(50) as usize;
    if limit == 0 {
        return Vec::new();
    }

    let mut rows = Vec::<(u32, TitleCard)>::new();
    let mut seen = HashSet::new();
    for block in document.select("div.borderClass").nodes().iter().cloned() {
        let selection = Selection::from(block);
        if !selection.select(".detail-user-recs-text").exists() {
            continue;
        }
        let Some((id, card)) = recommendation_card(&selection, manga) else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let votes = selection
            .select("a.js-similar-recommendations-button strong")
            .nodes()
            .first()
            .and_then(node_text)
            .and_then(|text| crate::model::count_from_text(&text))
            .map(|votes| votes.saturating_add(1))
            .unwrap_or(1);
        rows.push((votes, card));
    }

    rows.sort_by_key(|(votes, _)| std::cmp::Reverse(*votes));
    rows.into_iter()
        .take(limit)
        .map(|(_, title)| SimilarTitle {
            title,
            reason: Some("Рекомендуют пользователи MyAnimeList".into()),
        })
        .collect()
}

/// Fallback for the recommendation carousel embedded in the detail page.
pub fn recommendations_from_detail(
    document: &Document,
    manga: bool,
    limit: u32,
) -> Vec<SimilarTitle> {
    let limit = limit.min(50) as usize;
    if limit == 0 {
        return Vec::new();
    }
    let selector = if manga {
        "#manga_recommendation li.btn-anime, #manga_recommendation li, #anime_recommendation li.btn-anime, #anime_recommendation li"
    } else {
        "#anime_recommendation li.btn-anime, #anime_recommendation li, #manga_recommendation li.btn-anime, #manga_recommendation li"
    };
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for item in document.select(selector).nodes().iter().cloned() {
        let selection = Selection::from(item);
        let Some(link) = selection.select("a").nodes().first().cloned() else {
            continue;
        };
        let Some(href) = link.attr("href").map(|value| value.to_string()) else {
            continue;
        };
        let Some(id) = recommendation_id(&href) else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let entry = crate::model::EntryRef { id, manga };
        let name = selection
            .select("span.title")
            .nodes()
            .first()
            .and_then(node_text)
            .or_else(|| link.attr("title").map(|value| clean_text(&value)))
            .unwrap_or_else(|| format!("#{id}"));
        let poster = selection.select("img").nodes().first().and_then(image_url);
        result.push(SimilarTitle {
            title: TitleCard {
                id: entry.key(),
                name,
                alternative_names: Vec::new(),
                poster,
                media_type: entry.media_type(),
                format: None,
                status: AiringStatus::Unknown,
                year: None,
                tracker_data: tracker_links(&entry.key()),
                source_ref: None,
                url: Some(format!(
                    "https://myanimelist.net/{}/{}",
                    entry.kind(),
                    entry.id
                )),
            },
            reason: Some("Рекомендуют пользователи MyAnimeList".into()),
        });
        if result.len() >= limit {
            break;
        }
    }
    result
}

fn recommendation_card(selection: &Selection<'_>, manga: bool) -> Option<(u32, TitleCard)> {
    let link = find_title_link(selection, manga)?;
    let href = link.attr("href")?.to_string();
    let id = id_from_href(&href, if manga { "manga" } else { "anime" })?;
    let name = selection
        .select("a[href*='/anime/'] strong, a[href*='/manga/'] strong")
        .nodes()
        .first()
        .and_then(node_text)
        .or_else(|| node_text(&link))?;
    let entry = crate::model::EntryRef { id, manga };
    Some((
        id,
        TitleCard {
            id: entry.key(),
            name,
            alternative_names: Vec::new(),
            poster: selection
                .select("td img, .picSurround img")
                .nodes()
                .first()
                .and_then(image_url),
            media_type: if manga {
                MediaType::Manga
            } else {
                MediaType::Anime
            },
            format: None,
            status: AiringStatus::Unknown,
            year: None,
            tracker_data: tracker_links(&entry.key()),
            source_ref: None,
            url: Some(format!(
                "https://myanimelist.net/{}/{}",
                entry.kind(),
                entry.id
            )),
        },
    ))
}

fn find_title_link<'a>(selection: &Selection<'a>, manga: bool) -> Option<NodeRef<'a>> {
    let selector = if manga {
        "a[href*='/manga/'] strong, a[href*='/manga/']"
    } else {
        "a[href*='/anime/'] strong, a[href*='/anime/']"
    };
    selection
        .select(selector)
        .nodes()
        .iter()
        .cloned()
        .find_map(|node| {
            if node.has_name("strong") {
                node.parent()
            } else {
                Some(node)
            }
        })
        .filter(|link| {
            link.attr("href")
                .and_then(|href| {
                    id_from_href(&href.to_string(), if manga { "manga" } else { "anime" })
                })
                .is_some()
        })
}

fn recommendation_id(href: &str) -> Option<u32> {
    // `/recommendations/anime/20-34572` -> 34572
    if !href.contains("/recommendations/") {
        return None;
    }
    let tail = href.rsplit('/').next()?;
    let id = tail.rsplit('-').next()?;
    id.parse::<u32>().ok().filter(|id| *id > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::document;

    #[test]
    fn recommendations_are_sorted_deduplicated_and_keep_posters() {
        let doc = document(include_str!("../../tests/fixtures/recommendations.html"));
        let recommendations = recommendations(&doc, false, 50);
        assert_eq!(recommendations.len(), 2);
        assert_eq!(recommendations[0].title.id, "34572");
        assert_eq!(recommendations[0].title.name, "Black Clover");
        assert!(recommendations[0].title.poster.is_some());
        assert_eq!(recommendations[1].title.id, "21");
        assert!(recommendations
            .iter()
            .all(|item| item.title.tracker_data[0].name == "MyAnimeList"));
    }

    #[test]
    fn detail_carousel_uses_recommendation_permalink() {
        let document = document(
            r#"<html><body><div id="anime_recommendation"><ul>
            <li class="btn-anime" title="Black Clover"><a href="https://myanimelist.net/recommendations/anime/20-34572"><span class="title fs10">Black Clover</span><span class="users">60 Users</span><img data-src="https://cdn.myanimelist.net/r/90x140/images/anime/2/88336.webp"></a></li>
            </ul></div></body></html>"#,
        );
        let recommendations = recommendations_from_detail(&document, false, 50);
        assert_eq!(recommendations.len(), 1);
        assert_eq!(recommendations[0].title.id, "34572");
        assert!(recommendations[0].title.poster.is_some());
    }

    #[test]
    fn invalid_permalink_is_rejected() {
        assert_eq!(
            recommendation_id("/recommendations/anime/20-34572"),
            Some(34572)
        );
        assert_eq!(recommendation_id("/recommendations/anime/20-"), None);
        assert_eq!(recommendation_id("https://myanimelist.net/anime/20"), None);
    }
}
