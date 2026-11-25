//! MyAnimeList anime/manga detail page parser.
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/Parser/Anime/AnimeParser.php` — alternative titles, information rows,
//!   genres/themes/demographics, statistics, external links, relations,
//!   background, opening/ending themes and the trailer preview.
//! - `src/Parser/Manga/MangaParser.php` — manga spellings (`Published`,
//!   `Volumes`, `Chapters`, `Authors`, `Serialization`) and manga card fields.
//! - `src/Parser/Common/AnimeCardParser.php`, `MangaCardParser.php` — tiles
//!   inside Related Entries.
//! - `src/Parser/Common/MalUrlParser.php`, `UrlParser.php` — link/id handling.
//! - `src/Parser/Common/AlternativeTitleParser.php` — Synonyms/Japanese/English.
//! - `src/Parser/Anime/AnimeStatsParser.php` — score/rank/members parsing.
//! - `src/Helper/Constants.php` — `Unknown`, `N/A`, `None found` sentinels.
//!
//! MAL currently serves the same information with three different markups:
//! the modern right-column anime page, the older compact manga page, and the
//! relation tiles/table pair. Every lookup here is label- or class-based rather
//! than tied to a fixed `<td>` position, so a layout change degrades to a
//! missing field instead of a wrong value.

use crate::animex::module::types::{
    AiringStatus, AlternativeName, AnimePreview, InfoBlock, MediaType, RelatedTitle, LegacyRelease as Release,
    TitleCard, TitleRelation,
};
use dom_query::{Document, Selection};
use std::collections::HashSet;

use crate::html::{
    clean_text, image_url, label_count, label_links, label_text, node_text, normalize_image,
    select_attr, select_text,
};
use crate::model::{
    absolute_url, airing_status, anime_kind, card_format, episode_count, id_from_href,
    relation_code, relation_label, score_from_text, tracker_links, year_from_text, EntryRef,
};

pub fn release(document: &Document, entry: &EntryRef) -> Release {
    let name = title(document, entry);
    let aired = label_text(document, &["Aired", "Published"]);
    let premiered = label_text(document, &["Premiered"]);
    let broadcast = label_text(document, &["Broadcast"]);
    let source = label_text(document, &["Source"]);
    let duration = label_text(document, &["Duration"]);
    let rating_label = label_text(document, &["Rating"]);
    let rank = label_count(document, &["Ranked"]);
    let popularity = label_count(document, &["Popularity"]);
    let members = label_count(document, &["Members"]);
    let favorites = label_count(document, &["Favorites"]);
    let type_name = label_text(document, &["Type"]);
    let status = airing_status(label_text(document, &["Status"]).as_deref());
    let episodes = episode_count(label_text(document, &["Episodes"]).as_deref());
    let year = aired
        .as_deref()
        .and_then(year_from_text)
        .or_else(|| premiered.as_deref().and_then(year_from_text));

    let mut other_info = Vec::new();
    push_info(&mut other_info, "Выход", aired.clone());
    push_info(&mut other_info, "Сезон", premiered.clone());
    push_info(&mut other_info, "Трансляция", broadcast);
    push_info(&mut other_info, "Первоисточник", source);
    push_info(&mut other_info, "Длительность", duration);
    push_info(&mut other_info, "Возрастной рейтинг", rating_label);
    if entry.manga {
        push_info(
            &mut other_info,
            "Тома",
            label_text(document, &["Volumes"]).filter(|value| value != "Unknown"),
        );
        push_info(
            &mut other_info,
            "Главы",
            label_text(document, &["Chapters"]).filter(|value| value != "Unknown"),
        );
        push_info(
            &mut other_info,
            "Публикуется в",
            join_names(label_links(document, &["Serialization", "Serializations"])),
        );
        push_info(
            &mut other_info,
            "Авторы",
            join_names(label_links(
                document,
                &["Authors", "Author", "Story", "Art"],
            )),
        );
    } else {
        push_info(
            &mut other_info,
            "Продюсеры",
            join_names(label_links(document, &["Producers", "Producer"])),
        );
        push_info(
            &mut other_info,
            "Лицензиары",
            join_names(label_links(document, &["Licensors", "Licensor"])),
        );
    }
    push_info(
        &mut other_info,
        "Место в рейтинге",
        rank.map(|value| format!("#{value}")),
    );
    push_info(
        &mut other_info,
        "Популярность",
        popularity.map(|value| format!("#{value}")),
    );
    push_info(&mut other_info, "Участники", members.map(group_digits));
    push_info(&mut other_info, "В избранном", favorites.map(group_digits));
    let (openings, endings) = themes(document);
    push_list(&mut other_info, "Открывающие темы", openings);
    push_list(&mut other_info, "Закрывающие темы", endings);
    push_info(&mut other_info, "Фон", background(document));
    push_list(&mut other_info, "Ссылки", external_links(document, 12));

    Release {
        id: entry.key(),
        name: name.clone(),
        alternative_names: alternative_names(document, &name),
        tracker_data: entry.link(),
        other_info,
        banner_url: None,
        poster: poster(document),
        description: description(document),
        genres: genres(document),
        studios: label_links(document, &["Studios", "Studio"])
            .into_iter()
            .map(|(name, _)| name)
            .collect(),
        status,
        kind: anime_kind(type_name.as_deref(), entry.manga),
        year,
        trailer: trailer(document),
        total_episodes: if entry.manga { None } else { episodes },
        next_episode_at: None,
        rating: rating(document),
        source_ref: None,
    }
}

/// Search previews come from the search results page, not from a detail page.
/// Kept for callers that need a preview while a detail page is already cached.
#[allow(dead_code)]
pub fn extended_release(document: &Document, entry: &EntryRef) -> crate::animex::module::types::Release {
    let mut result = crate::release_metadata::upgrade(release(document, entry));
    if entry.manga {
        return result;
    }
    let aired = label_text(document, &["Aired"]).unwrap_or_default();
    let (from, to) = aired.split_once(" to ").unwrap_or((&aired, ""));
    result.aired_from = english_date(from);
    result.aired_to = english_date(to);
    result.episode_duration_seconds = label_text(document, &["Duration"])
        .as_deref().and_then(crate::release_metadata::duration_text);
    result.age_rating = label_text(document, &["Rating"]).as_deref()
        .and_then(|value| crate::release_metadata::age_rating(value.split(" - ").next()?));
    result.original_source = label_text(document, &["Source"]).as_deref()
        .and_then(crate::release_metadata::original_source);
    result.score_scale = result.rating.map(|_| 10);
    result.url = absolute_url(&entry.path(None));
    let links = label_links(document, &["Genres", "Genre"]);
    for genre in &mut result.genres {
        genre.language = Some("en".into());
        if let Some(href) = links.iter().find(|(name, _)| name == &genre.name).and_then(|(_, href)| href.as_deref()) {
            genre.url = absolute_url(href);
            genre.id = href.split("/genre/").nth(1).and_then(|path| path.split('/').next())
                .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).map(str::to_owned);
        }
    }
    result
}

fn english_date(value: &str) -> Option<crate::animex::module::types::CalendarDate> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let parts: Vec<_> = value.split_whitespace().collect();
    let year = parts.last()?.parse().ok()?;
    let month = MONTHS.iter().position(|name| Some(*name) == parts.first().copied()).map(|i| i as i32 + 1);
    let day = (parts.len() == 3).then(|| parts[1].trim_end_matches(',').parse().ok()).flatten();
    crate::release_metadata::date(year, month, day)
}

pub fn preview_from_detail(document: &Document, entry: &EntryRef) -> AnimePreview {
    let name = title(document, entry);
    AnimePreview {
        id: entry.key(),
        title: name.clone(),
        alternative_names: alternative_names(document, &name),
        genres: genres(document),
        description: description(document),
        poster: poster(document),
        year: label_text(document, &["Aired", "Published"])
            .or_else(|| label_text(document, &["Premiered"]))
            .as_deref()
            .and_then(year_from_text),
        score: rating(document),
        kind: anime_kind(label_text(document, &["Type"]).as_deref(), entry.manga),
        status: airing_status(label_text(document, &["Status"]).as_deref()),
        episodes_count: if entry.manga {
            None
        } else {
            episode_count(label_text(document, &["Episodes"]).as_deref())
        },
        tracker_data: entry.link(),
        source_ref: None,
    }
}

/// Related titles are present on the detail page in two shapes: image tiles
/// (`div.entries-tile .entry`) and a compact table (`table.entries-table`).
pub fn related(document: &Document) -> Vec<RelatedTitle> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();

    for entry_node in document
        .select(".related-entries .entries-tile .entry")
        .nodes()
        .iter()
        .cloned()
    {
        let selection = Selection::from(entry_node);
        let relation_text = clean_text(&selection.select(".relation").text());
        let (relation_name, format_hint) = split_relation(&relation_text);
        let Some(link) = selection.select(".title a").nodes().first().cloned() else {
            continue;
        };
        let Some(href) = link.attr("href").map(|value| value.to_string()) else {
            continue;
        };
        let Some(title) = node_text(&link) else {
            continue;
        };
        let Some(card) = relation_card(
            &href,
            &title,
            selection
                .select(".image img")
                .nodes()
                .first()
                .and_then(image_url),
            format_hint.as_deref(),
        ) else {
            continue;
        };
        let relation = relation_code(&relation_name);
        if seen.insert((card.id.clone(), relation.clone())) {
            result.push(RelatedTitle {
                title: card,
                relation: TitleRelation {
                    label: relation_label(&relation).to_string(),
                    code: relation,
                },
            });
        }
    }

    for row in document
        .select("table.entries-table tr")
        .nodes()
        .iter()
        .cloned()
    {
        let selection = Selection::from(row);
        let relation_name = clean_text(&selection.select("td:first-child").text())
            .trim_end_matches(':')
            .to_string();
        if relation_name.is_empty() {
            continue;
        }
        let relation = relation_code(&relation_name);
        for link in selection
            .select("td:nth-child(2) a")
            .nodes()
            .iter()
            .cloned()
        {
            let Some(href) = link.attr("href").map(|value| value.to_string()) else {
                continue;
            };
            let Some(title) = node_text(&link) else {
                continue;
            };
            let parent_text = link
                .parent()
                .map(|parent| clean_text(&parent.text()))
                .unwrap_or_default();
            let format_hint = parenthetical(&parent_text);
            let Some(card) = relation_card(&href, &title, None, format_hint.as_deref()) else {
                continue;
            };
            if seen.insert((card.id.clone(), relation.clone())) {
                result.push(RelatedTitle {
                    title: card,
                    relation: TitleRelation {
                        label: relation_label(&relation).to_string(),
                        code: relation.clone(),
                    },
                });
            }
        }
    }

    result
}

pub fn title(document: &Document, entry: &EntryRef) -> String {
    select_attr(document, "meta[property='og:title']", "content")
        .or_else(|| select_text(document, "h1.title-name strong"))
        .or_else(|| select_text(document, "h1.title-name"))
        .or_else(|| select_text(document, ".h1"))
        .unwrap_or_else(|| format!("#{}", entry.id))
}

/// Id advertised by the page's canonical/OG URL. Used to reject a redirected
/// or cached page that belongs to a different MAL entry.
pub fn page_id(document: &Document, manga: bool) -> Option<u32> {
    let url = select_attr(document, "meta[property='og:url']", "content")
        .or_else(|| select_attr(document, "link[rel='canonical']", "href"))?;
    id_from_href(&url, if manga { "manga" } else { "anime" })
}

pub fn poster(document: &Document) -> Option<String> {
    select_attr(document, "meta[property='og:image']", "content")
        .and_then(|value| normalize_image(&value))
        .or_else(|| {
            document
                .select(".leftside img, .release-poster img")
                .nodes()
                .iter()
                .cloned()
                .find_map(|node| image_url(&node))
        })
}

pub fn description(document: &Document) -> Option<String> {
    select_text(
        document,
        "p[itemprop='description'], span[itemprop='description']",
    )
    .filter(|text| !text.starts_with("No synopsis information has been added to this title."))
}

pub fn alternative_names(document: &Document, main: &str) -> Vec<AlternativeName> {
    let mut result = Vec::new();
    let mut seen = HashSet::from([main.trim().to_lowercase()]);
    let mut push = |value: Option<String>, language: &str| {
        let Some(value) = value else {
            return;
        };
        let value = clean_text(&value);
        if value.is_empty() {
            return;
        }
        if seen.insert(value.to_lowercase()) {
            result.push(AlternativeName {
                name: value,
                language: language.to_string(),
            });
        }
    };

    push(label_text(document, &["English"]), "en");
    push(label_text(document, &["Japanese"]), "ja");
    if let Some(synonyms) = label_text(document, &["Synonyms"]) {
        for synonym in synonyms.split(',') {
            push(Some(synonym.to_string()), "unknown");
        }
    }
    result
}

pub fn genres(document: &Document) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for labels in [
        &["Genres", "Genre"][..],
        &["Explicit Genres", "Explicit Genre"][..],
        &["Themes", "Theme"][..],
        &["Demographics", "Demographic"][..],
    ] {
        for (name, _) in label_links(document, labels) {
            if seen.insert(name.to_lowercase()) {
                result.push(name);
            }
        }
    }
    result
}

pub fn rating(document: &Document) -> Option<f32> {
    select_text(document, "span[itemprop='ratingValue']")
        .and_then(|text| first_score(&text))
        .or_else(|| label_text(document, &["Score"]).and_then(|text| first_score(&text)))
}

pub fn trailer(document: &Document) -> Option<String> {
    select_attr(document, "div.video-promotion a", "href")
        .or_else(|| select_attr(document, "a.js-video-promotion", "href"))
        .or_else(|| select_attr(document, "meta[property='og:video']", "content"))
        .and_then(|href| absolute_url(&href))
}

pub fn themes(document: &Document) -> (Vec<String>, Vec<String>) {
    let mut openings = Vec::new();
    let mut endings = Vec::new();
    for container in document.select("div.theme-songs").nodes().iter().cloned() {
        let is_opening = container.has_class("opnening") || container.has_class("opening");
        let is_ending = container.has_class("ending");
        if !is_opening && !is_ending {
            continue;
        }
        let target = if is_opening {
            &mut openings
        } else {
            &mut endings
        };
        let selection = Selection::from(container);
        for row in selection.select("table tr").nodes().iter().cloned() {
            let text = strip_theme_index(&clean_text(&row.text()));
            if text.is_empty()
                || text.contains("No opening themes have been added")
                || text.contains("No ending themes have been added")
            {
                continue;
            }
            if !target.contains(&text) {
                target.push(text);
            }
        }
    }
    (openings, endings)
}

pub fn background(document: &Document) -> Option<String> {
    let heading = document.select("h2").nodes().iter().cloned().find(|node| {
        let text = clean_text(&node.text());
        text.eq_ignore_ascii_case("Background")
    })?;
    let mut sibling = heading.parent()?.next_element_sibling()?;
    loop {
        if sibling.has_name("h2") || sibling.has_name("h3") {
            return None;
        }
        let text = clean_text(&sibling.text());
        if !text.is_empty() {
            if text.contains("No background information has been added to this title") {
                return None;
            }
            return Some(text);
        }
        sibling = sibling.next_element_sibling()?;
    }
}

fn external_links(document: &Document, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    document
        .select("div.external_links a.link")
        .nodes()
        .iter()
        .cloned()
        .filter_map(|link| {
            let href = absolute_url(&link.attr("href")?.to_string())?;
            if !seen.insert(href.clone()) {
                return None;
            }
            let caption = clean_text(
                &Selection::from(link.clone())
                    .select_single(".caption")
                    .text(),
            );
            let caption = if caption.is_empty() {
                "Ссылка".to_string()
            } else {
                caption
            };
            Some(format!("{caption}: {href}"))
        })
        .take(limit)
        .collect()
}

fn relation_card(
    href: &str,
    name: &str,
    poster: Option<String>,
    format_hint: Option<&str>,
) -> Option<TitleCard> {
    let (media_type, id) = if let Some(id) = id_from_href(href, "manga") {
        (MediaType::Manga, id)
    } else if let Some(id) = id_from_href(href, "anime") {
        (MediaType::Anime, id)
    } else {
        return None;
    };
    let entry = EntryRef {
        id,
        manga: matches!(media_type, MediaType::Manga),
    };
    Some(TitleCard {
        id: entry.key(),
        name: clean_text(name),
        alternative_names: Vec::new(),
        poster,
        media_type,
        format: card_format(format_hint),
        status: AiringStatus::Unknown,
        year: None,
        tracker_data: tracker_links(&entry.key()),
        source_ref: None,
        url: absolute_url(href),
    })
}

fn split_relation(raw: &str) -> (String, Option<String>) {
    let raw = clean_text(raw);
    if let Some(open) = raw.rfind('(') {
        if raw.ends_with(')') {
            let name = raw[..open].trim().trim_end_matches(':').to_string();
            let hint = raw[open + 1..raw.len() - 1].trim().to_string();
            return (name, (!hint.is_empty()).then_some(hint));
        }
    }
    (raw.trim_end_matches(':').trim().to_string(), None)
}

fn parenthetical(raw: &str) -> Option<String> {
    let open = raw.rfind('(')?;
    let close = raw.rfind(')')?;
    (close > open).then(|| raw[open + 1..close].trim().to_string())
}

fn first_score(text: &str) -> Option<f32> {
    text.split_whitespace()
        .next()
        .and_then(|value| score_from_text(value))
}

fn strip_theme_index(text: &str) -> String {
    text.trim_start_matches(|character: char| {
        character.is_ascii_digit() || character == ':' || character == ' '
    })
    .to_string()
}

fn join_names(links: Vec<(String, Option<String>)>) -> Option<String> {
    let names: Vec<String> = links.into_iter().map(|(name, _)| name).collect();
    (!names.is_empty()).then(|| names.join(", "))
}

fn push_info(target: &mut Vec<InfoBlock>, title: &str, value: Option<String>) {
    let Some(value) = value else {
        return;
    };
    let value = clean_text(&value);
    if value.is_empty() {
        return;
    }
    target.push(InfoBlock {
        title: title.to_string(),
        data: vec![value],
    });
}

fn push_list(target: &mut Vec<InfoBlock>, title: &str, values: Vec<String>) {
    if values.is_empty() {
        return;
    }
    target.push(InfoBlock {
        title: title.to_string(),
        data: values,
    });
}

fn group_digits(value: u32) -> String {
    let digits = value.to_string();
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            result.push(' ');
        }
        result.push(character);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::document;

    fn page() -> Document {
        document(include_str!("../../tests/fixtures/anime.html"))
    }

    #[test]
    fn release_reads_every_information_row() {
        let entry = EntryRef::parse("20").unwrap();
        let release = release(&page(), &entry);
        assert_eq!(release.id, "20");
        assert_eq!(release.name, "Naruto");
        assert_eq!(release.year, Some(2002));
        assert_eq!(release.total_episodes, Some(220));
        assert_eq!(release.rating, Some(8.03));
        assert!(matches!(
            release.status,
            AiringStatus::Ongoing | AiringStatus::Released
        ));
        assert_eq!(release.kind.as_deref(), Some("tv"));
        assert!(release.poster.as_deref().unwrap().ends_with(".jpg"));
        assert!(release.description.as_deref().unwrap().contains("ninja"));
        assert_eq!(
            release.genres,
            vec!["Action", "Adventure", "Horror", "Martial Arts", "Shounen"]
        );
        assert_eq!(release.studios, vec!["Studio Pierrot"]);
        assert!(release
            .alternative_names
            .iter()
            .any(|name| name.language == "ja"));
        let titles: Vec<&str> = release
            .other_info
            .iter()
            .map(|block| block.title.as_str())
            .collect();
        for expected in [
            "Выход",
            "Сезон",
            "Трансляция",
            "Первоисточник",
            "Длительность",
            "Возрастной рейтинг",
            "Продюсеры",
            "Место в рейтинге",
            "Участники",
            "Открывающие темы",
            "Ссылки",
        ] {
            assert!(titles.contains(&expected), "missing {expected}: {titles:?}");
        }
    }

    #[test]
    fn related_merges_tiles_and_table_without_duplicates() {
        let related = related(&page());
        let ids: Vec<&str> = related.iter().map(|item| item.title.id.as_str()).collect();
        assert!(ids.contains(&"1735"));
        assert!(ids.contains(&"manga:11"));
        assert_eq!(
            related
                .iter()
                .filter(|item| item.title.id == "manga:11")
                .count(),
            1
        );
        assert!(related
            .iter()
            .any(|item| item.relation.code == "adaptation" && item.relation.label == "Адаптация"));
        let manga = related
            .iter()
            .find(|item| item.title.id == "manga:11")
            .unwrap();
        assert!(matches!(manga.title.media_type, MediaType::Manga));
        assert_eq!(manga.title.format.as_deref(), Some("manga"));
    }

    #[test]
    fn themes_and_background_are_not_taken_from_neighbour_sections() {
        let (openings, endings) = themes(&page());
        assert_eq!(openings.len(), 2);
        assert!(openings[0].contains("ROCKS"));
        assert_eq!(endings.len(), 1);
        assert!(endings[0].contains("Wind"));
        assert!(background(&page()).unwrap().contains("awards"));
    }

    #[test]
    fn manga_release_does_not_become_anime_and_keeps_publishing_fields() {
        let document = document(include_str!("../../tests/fixtures/manga.html"));
        let entry = EntryRef::parse("manga:2").unwrap();
        let release = release(&document, &entry);
        assert_eq!(release.id, "manga:2");
        assert_eq!(release.name, "Berserk");
        assert!(release.kind.is_none());
        assert!(release.total_episodes.is_none());
        assert_eq!(release.year, Some(1989));
        assert_eq!(release.rating, Some(9.46));
        assert!(matches!(release.status, AiringStatus::Ongoing));
        let titles: Vec<&str> = release
            .other_info
            .iter()
            .map(|block| block.title.as_str())
            .collect();
        for expected in ["Тома", "Главы", "Авторы", "Публикуется в"] {
            assert!(titles.contains(&expected), "missing {expected}: {titles:?}");
        }
        let related = related(&document);
        assert_eq!(related[0].title.id, "33");
        assert!(matches!(related[0].title.media_type, MediaType::Anime));
        assert_eq!(related[0].relation.code, "adaptation");
    }

    #[test]
    fn preview_matches_release_identity_and_metadata() {
        let entry = EntryRef::parse("20").unwrap();
        let preview = preview_from_detail(&page(), &entry);
        let release = release(&page(), &entry);
        assert_eq!(preview.id, release.id);
        assert_eq!(preview.title, release.name);
        assert_eq!(preview.score, release.rating);
        assert_eq!(preview.episodes_count, release.total_episodes);
        assert_eq!(preview.status, release.status);
        assert_eq!(preview.tracker_data[0].data, "20");
    }
}
