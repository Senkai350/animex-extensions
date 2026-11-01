//! Anime opening/ending and promotional videos.
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/Parser/Anime/VideosParser.php` — `video-block music-video` and
//!   `video-block promotional-video` sections; episode videos are a separate
//!   resource and are intentionally not mapped to tracker videos.
//! - `src/Parser/Anime/PromoListItemParser.php` — `a[href]`, `img[data-src]`
//!   and `span.title`.
//! - `src/Parser/Anime/MusicVideoListItemParser.php` — title plus the
//!   `"Song" by Artist` line; MAL embeds YouTube via `youtube-nocookie.com`.
//! - `src/Request/Anime/AnimeVideosRequest.php` — `/anime/<id>/_/video`.
//!
//! MAL exposes embed URLs. The module stores ordinary watch URLs so opening a
//! video in the system browser works without an iframe wrapper.

use crate::animex::module::types::{TrackerVideo, VideoKind};
use dom_query::{Document, NodeRef, Selection};
use std::collections::HashSet;

use crate::html::{clean_text, image_url, node_text};
use crate::model::absolute_url;

pub fn videos(document: &Document) -> Vec<TrackerVideo> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();

    for item in document
        .select(
            "div.video-block.music-video section .video-list-outer, \
             div.video-block.music-video .video-list-outer",
        )
        .nodes()
        .iter()
        .cloned()
    {
        if let Some(video) = parse_music(&item) {
            if seen.insert(video.url.clone()) {
                result.push(video);
            }
        }
    }

    for item in document
        .select(
            "div.video-block.promotional-video section .video-list-outer, \
             div.video-block.promotional-video .video-list-outer",
        )
        .nodes()
        .iter()
        .cloned()
    {
        if let Some(video) = parse_promo(&item) {
            if seen.insert(video.url.clone()) {
                result.push(video);
            }
        }
    }

    result
}

/// Detail-page fallback: MAL sometimes embeds a trailer in `.video-promotion`
/// even when the dedicated video page has no promotional entries.
pub fn videos_from_detail(document: &Document) -> Vec<TrackerVideo> {
    document
        .select("div.video-promotion a")
        .nodes()
        .iter()
        .cloned()
        .filter_map(|link| {
            let url = absolute_url(&link.attr("href")?.to_string())?;
            let name = node_text(&link).unwrap_or_else(|| "Трейлер".to_string());
            Some(TrackerVideo {
                id: url.clone(),
                kind: VideoKind::Trailer,
                name: Some(name),
                url,
                thumbnail: None,
            })
        })
        .collect()
}

fn parse_music(item: &NodeRef<'_>) -> Option<TrackerVideo> {
    let selection = Selection::from(item.clone());
    let link = selection
        .select("a.iframe, a[href]")
        .nodes()
        .first()?
        .clone();
    let raw_url = link.attr("href")?.to_string();
    let url = youtube_watch_url(&raw_url).or_else(|| absolute_url(&raw_url))?;
    let title = selection
        .select("span.title")
        .nodes()
        .first()
        .and_then(node_text)
        .or_else(|| link.attr("data-title").map(|value| clean_text(&value)))
        .unwrap_or_else(|| "Видео".to_string());
    let music = clean_text(&selection.select("a + div").text());
    let name = if music.is_empty() {
        title.clone()
    } else {
        format!("{title} — {music}")
    };
    let kind = if title.to_ascii_lowercase().starts_with("op")
        || title.to_ascii_lowercase().contains("opening")
    {
        VideoKind::Opening
    } else if title.to_ascii_lowercase().starts_with("ed")
        || title.to_ascii_lowercase().contains("ending")
    {
        VideoKind::Ending
    } else {
        VideoKind::Other
    };
    Some(TrackerVideo {
        id: youtube_id(&raw_url).unwrap_or_else(|| url.clone()),
        kind,
        name: Some(name),
        url,
        thumbnail: selection.select("img").nodes().first().and_then(image_url),
    })
}

fn parse_promo(item: &NodeRef<'_>) -> Option<TrackerVideo> {
    let selection = Selection::from(item.clone());
    let link = selection
        .select("a.iframe, a[href]")
        .nodes()
        .first()?
        .clone();
    let raw_url = link.attr("href")?.to_string();
    let url = youtube_watch_url(&raw_url).or_else(|| absolute_url(&raw_url))?;
    let title = selection
        .select("span.title")
        .nodes()
        .first()
        .and_then(node_text)
        .or_else(|| link.attr("data-title").map(|value| clean_text(&value)))
        .unwrap_or_else(|| "Промо".to_string());
    let lower = title.to_ascii_lowercase();
    let kind = if lower.contains("trailer") || lower.contains("трейлер") {
        VideoKind::Trailer
    } else {
        VideoKind::Pv
    };
    Some(TrackerVideo {
        id: youtube_id(&raw_url).unwrap_or_else(|| url.clone()),
        kind,
        name: Some(title),
        url,
        thumbnail: selection.select("img").nodes().first().and_then(image_url),
    })
}

fn youtube_id(raw: &str) -> Option<String> {
    let value = raw.trim();
    for marker in [
        "youtube-nocookie.com/embed/",
        "youtube.com/embed/",
        "youtu.be/",
    ] {
        if let Some(index) = value.find(marker) {
            let rest = &value[index + marker.len()..];
            let id: String = rest
                .chars()
                .take_while(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
                .collect();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    if let Some(index) = value.find("v=") {
        let rest = &value[index + 2..];
        let id: String = rest
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
            .collect();
        if !id.is_empty() {
            return Some(id);
        }
    }
    None
}

fn youtube_watch_url(raw: &str) -> Option<String> {
    youtube_id(raw).map(|id| format!("https://www.youtube.com/watch?v={id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::document;

    #[test]
    fn music_and_promos_are_mapped_without_episode_streams() {
        let doc = document(include_str!("../../tests/fixtures/videos.html"));
        let videos = videos(&doc);
        assert_eq!(videos.len(), 3);
        assert!(matches!(videos[0].kind, VideoKind::Opening));
        assert!(matches!(videos[1].kind, VideoKind::Ending));
        assert!(matches!(videos[2].kind, VideoKind::Trailer));
        assert_eq!(videos[0].url, "https://www.youtube.com/watch?v=XggsuWPjLhc");
        assert_eq!(videos[0].id, "XggsuWPjLhc");
        assert!(videos[0].name.as_deref().unwrap().contains("ROCKS"));
        assert!(videos[0].thumbnail.is_some());
    }

    #[test]
    fn duplicate_urls_are_removed_and_empty_pages_return_nothing() {
        let doc = document(include_str!("../../tests/fixtures/videos.html"));
        let parsed = videos(&doc);
        let urls: Vec<&str> = parsed.iter().map(|video| video.url.as_str()).collect();
        let unique: HashSet<&str> = urls.iter().copied().collect();
        assert_eq!(urls.len(), unique.len());
        assert!(videos(&document(
            r#"<html><body><div class="video-block promotional-video"><p>No promotional video has been added to this title.</p></div></body></html>"#
        ))
        .is_empty());
    }

    #[test]
    fn youtube_embed_and_watch_urls_share_one_id() {
        assert_eq!(
            youtube_id("https://www.youtube-nocookie.com/embed/XggsuWPjLhc?enablejsapi=1"),
            Some("XggsuWPjLhc".into())
        );
        assert_eq!(
            youtube_id("https://youtu.be/XggsuWPjLhc"),
            Some("XggsuWPjLhc".into())
        );
        assert_eq!(
            youtube_id("https://www.youtube.com/watch?v=XggsuWPjLhc"),
            Some("XggsuWPjLhc".into())
        );
    }

    #[test]
    fn detail_trailer_is_a_fallback() {
        let document = document(
            r#"<html><body><div class="video-promotion"><a href="https://www.youtube.com/watch?v=abc">PV</a></div></body></html>"#,
        );
        let videos = videos_from_detail(&document);
        assert_eq!(videos.len(), 1);
        assert!(matches!(videos[0].kind, VideoKind::Trailer));
    }
}
