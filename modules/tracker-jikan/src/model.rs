//! Shared identity and value mapping for the MyAnimeList tracker.
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/MyAnimeList/MalClient.php` — anime/manga id handling and 404 semantics.
//! - `src/Model/Common/MalUrl.php` — MAL URL and id representation.
//! - `src/Model/Anime/Anime.php`, `src/Model/Manga/Manga.php` — release fields.
//! - `src/Helper/Constants.php` — status/type display values.
//! - `src/Helper/Time.php` — date helpers (we only need the start year here).
//!
//! The host contract uses `manga:<id>` for manga so numeric anime and manga ids
//! never collide. Anime ids stay bare (`20`), exactly as the previous
//! jikan-edge implementation and existing library links expect.

use crate::animex::module::types::{AiringStatus, MediaType, TrackerData};

pub const SITE: &str = "https://myanimelist.net";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRef {
    pub id: u32,
    pub manga: bool,
}

impl EntryRef {
    /// Same validation rules as the old module: positive decimal id only.
    /// Paths, query strings, signs and zero are rejected before any request.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        let (manga, value) = raw
            .strip_prefix("manga:")
            .map(|rest| (true, rest))
            .unwrap_or((false, raw));
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("MyAnimeList: invalid title id".into());
        }
        let id = value
            .parse::<u32>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or_else(|| "MyAnimeList: invalid title id".to_string())?;
        Ok(Self { id, manga })
    }

    pub fn key(&self) -> String {
        if self.manga {
            format!("manga:{}", self.id)
        } else {
            self.id.to_string()
        }
    }

    pub fn kind(&self) -> &'static str {
        if self.manga {
            "manga"
        } else {
            "anime"
        }
    }

    /// MAL accepts `/anime/<id>` without a slug; this avoids an extra redirect
    /// and keeps cache keys stable when the title is renamed.
    pub fn path(&self, section: Option<&str>) -> String {
        match section {
            // MAL needs a slug segment for subpages; `_` is the placeholder
            // used by jikan-me/jikan (e.g. `/anime/20/_/characters`). Without
            // it MAL can answer with the detail page instead of the section.
            Some(section) => format!("/{}/{}/_/{}", self.kind(), self.id, section),
            None => format!("/{}/{}", self.kind(), self.id),
        }
    }

    pub fn page_key(&self, section: Option<&str>) -> String {
        match section {
            Some(section) => format!("mal:v1:page:{}:{}:{}", self.kind(), self.id, section),
            None => format!("mal:v1:page:{}:{}", self.kind(), self.id),
        }
    }

    pub fn link(&self) -> Vec<TrackerData> {
        tracker_links(&self.key())
    }

    pub fn media_type(&self) -> MediaType {
        if self.manga {
            MediaType::Manga
        } else {
            MediaType::Anime
        }
    }
}

/// App convention: MAL entries also expose the corresponding Shikimori key.
/// Keep the media prefix so anime and manga cannot collide.
pub fn tracker_links(key: &str) -> Vec<TrackerData> {
    vec![
        TrackerData {
            name: "MyAnimeList".into(),
            data: key.into(),
        },
        TrackerData {
            name: "Shikimori".into(),
            data: key.into(),
        },
    ]
}

/// MAL status strings, including the manga spellings. On hiatus is still an
/// ongoing title for the UI (new episodes may resume), matching the mapper in
/// `src/Helper/Constants.php`.
pub fn airing_status(raw: Option<&str>) -> AiringStatus {
    match raw.unwrap_or_default().trim() {
        "Currently Airing" | "Publishing" | "On Hiatus" => AiringStatus::Ongoing,
        "Finished Airing" | "Finished" | "Discontinued" => AiringStatus::Released,
        "Not yet aired" | "Not yet published" => AiringStatus::Anons,
        _ => AiringStatus::Unknown,
    }
}

/// Canonical anime kinds understood by the host (`anime-preview.kind`).
pub fn anime_kind(raw: Option<&str>, manga: bool) -> Option<String> {
    if manga {
        return None;
    }
    let kind = raw?.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    matches!(
        kind.as_str(),
        "tv" | "movie" | "ova" | "ona" | "special" | "tv_special" | "music" | "pv" | "cm"
    )
    .then_some(kind)
}

/// `TitleCard.format` accepts a broader vocabulary than release `kind`, so
/// manga formats (light novel, one-shot, manhwa, ...) are preserved.
pub fn card_format(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_ascii_lowercase().replace([' ', '-'], "_"))
}

/// First plausible four-digit year in a MAL date range. "Aug 25, 1989 to ?"
/// yields 1989; "Fall 2002" yields 2002; "?" yields None.
pub fn year_from_text(raw: &str) -> Option<u16> {
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index + 4 <= bytes.len() {
        if bytes[index..index + 4].iter().all(u8::is_ascii_digit) {
            let before_ok = index == 0 || !bytes[index - 1].is_ascii_digit();
            let after_ok = index + 4 == bytes.len() || !bytes[index + 4].is_ascii_digit();
            if before_ok && after_ok {
                if let Ok(year) = raw[index..index + 4].parse::<u16>() {
                    if (1900..=2100).contains(&year) {
                        return Some(year);
                    }
                }
            }
            index += 4;
        } else {
            index += 1;
        }
    }
    None
}

/// Parse a MAL score. `N/A`, `0` and out-of-range values mean "no score".
pub fn score_from_text(raw: &str) -> Option<f32> {
    let value = raw.trim().replace(',', ".");
    let score = value.parse::<f32>().ok()?;
    (score.is_finite() && score > 0.0 && score <= 10.0).then_some(score)
}

/// Parse the first integer in a MAL count/label. Handles thousands separators
/// (`3,173,820`) and ignores a trailing footnote (`#7272`).
pub fn count_from_text(raw: &str) -> Option<u32> {
    let digits: String = raw
        .char_indices()
        .skip_while(|(_, ch)| !ch.is_ascii_digit())
        .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == ',' || ch.is_ascii_whitespace())
        .filter_map(|(_, ch)| ch.is_ascii_digit().then_some(ch))
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

/// MAL `Episodes: Unknown` / `0` / `?` must not become `Some(0)`.
pub fn episode_count(raw: Option<&str>) -> Option<u32> {
    let raw = raw?.trim();
    if raw.eq_ignore_ascii_case("unknown") || raw == "?" || raw.is_empty() {
        return None;
    }
    count_from_text(raw).filter(|count| *count > 0)
}

/// Convert an absolute or site-relative MAL link into a stable URL.
pub fn absolute_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with("javascript:") || raw.starts_with('#') {
        return None;
    }
    if raw.starts_with("https://") || raw.starts_with("http://") {
        return Some(raw.to_string());
    }
    if let Some(rest) = raw.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    if raw.starts_with('/') {
        return Some(format!("{SITE}{raw}"));
    }
    None
}

/// Extract the numeric id out of `/anime/20/...` or `/manga/2/...`.
pub fn id_from_href(href: &str, kind: &str) -> Option<u32> {
    let marker = format!("/{kind}/");
    let start = href.find(&marker)? + marker.len();
    let digits: String = href[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok().filter(|id| *id > 0)
}

/// Relation type from MAL's "Sequel (TV)" / "Side Story:" labels.
pub fn relation_code(raw: &str) -> String {
    raw.trim()
        .trim_end_matches(':')
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
}

pub fn relation_label(code: &str) -> &str {
    match code {
        "prequel" => "Предыстория",
        "sequel" => "Продолжение",
        "adaptation" => "Адаптация",
        "source" => "Первоисточник",
        "parent_story" => "Основная история",
        "side_story" => "Побочная история",
        "spin_off" => "Спин-офф",
        "alternative_version" => "Альтернативная версия",
        "alternative_setting" => "Альтернативная вселенная",
        "summary" => "Краткое изложение",
        "full_story" => "Полная история",
        "character" => "Персонажи",
        "other" => "Другое",
        _ => code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_keep_namespaces_and_reject_paths() {
        for raw in [
            "",
            "0",
            "-1",
            "1/characters",
            "anime:1",
            "manga:0",
            "manga:1?x=1",
            "4294967296",
        ] {
            assert!(EntryRef::parse(raw).is_err(), "{raw}");
        }
        let anime = EntryRef::parse("0020").unwrap();
        assert_eq!(anime.key(), "20");
        assert_eq!(anime.path(None), "/anime/20");
        assert_eq!(anime.path(Some("characters")), "/anime/20/_/characters");
        assert_eq!(
            anime.page_key(Some("videos")),
            "mal:v1:page:anime:20:videos"
        );
        let manga = EntryRef::parse("manga:2").unwrap();
        assert_eq!(manga.key(), "manga:2");
        assert_eq!(manga.path(Some("stats")), "/manga/2/_/stats");
    }

    #[test]
    fn dates_scores_and_counts_are_not_guessed() {
        assert_eq!(year_from_text("Aug 25, 1989 to ?"), Some(1989));
        assert_eq!(year_from_text("Fall 2002"), Some(2002));
        assert_eq!(year_from_text("Not available"), None);
        assert_eq!(year_from_text("1800"), None);
        assert_eq!(score_from_text("8.03"), Some(8.03));
        assert_eq!(score_from_text("N/A"), None);
        assert_eq!(score_from_text("0"), None);
        assert_eq!(score_from_text("11"), None);
        assert_eq!(count_from_text("3,173,820"), Some(3_173_820));
        assert_eq!(count_from_text("#7272"), Some(7272));
        assert_eq!(count_from_text("Unknown"), None);
        assert_eq!(episode_count(Some("Unknown")), None);
        assert_eq!(episode_count(Some("220")), Some(220));
    }

    #[test]
    fn statuses_and_kinds_match_the_host_vocabulary() {
        assert!(matches!(
            airing_status(Some("Currently Airing")),
            AiringStatus::Ongoing
        ));
        assert!(matches!(
            airing_status(Some("Finished")),
            AiringStatus::Released
        ));
        assert!(matches!(
            airing_status(Some("Not yet aired")),
            AiringStatus::Anons
        ));
        assert_eq!(
            anime_kind(Some("TV Special"), false).as_deref(),
            Some("tv_special")
        );
        assert_eq!(anime_kind(Some("Light Novel"), true), None);
        assert_eq!(
            card_format(Some("Light Novel")).as_deref(),
            Some("light_novel")
        );
    }

    #[test]
    fn links_and_relations_are_normalized() {
        assert_eq!(
            absolute_url("/anime/20/Naruto").as_deref(),
            Some("https://myanimelist.net/anime/20/Naruto")
        );
        assert_eq!(absolute_url("javascript:void(0)"), None);
        assert_eq!(
            id_from_href("https://myanimelist.net/anime/20/Naruto", "anime"),
            Some(20)
        );
        assert_eq!(id_from_href("/manga/2/Berserk", "manga"), Some(2));
        assert_eq!(relation_code("Side Story:"), "side_story");
        assert_eq!(relation_label("side_story"), "Побочная история");
    }
}
