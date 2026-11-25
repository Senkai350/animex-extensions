use crate::animex::module::types::{Genre, RegionalPremiere, Release};
use crate::{absolutize, gql, release, release_metadata, retry_rate_limited, AnimeNode, SITE};
use dom_query::{Document, Selection};
use serde::Deserialize;

const QUERY: &str = r#"
query AnimeDetails($id: String!) {
  animes(ids: $id, limit: 1) {
    id malId url name russian english japanese synonyms description
    kind status score episodes duration rating origin
    airedOn { year month day }
    releasedOn { year month day }
    poster { originalUrl previewUrl }
    genres { id name russian kind }
  }
}
"#;

#[derive(Deserialize)]
struct DetailsData {
    animes: Vec<DetailsNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsNode {
    #[serde(flatten)]
    anime: AnimeNode,
    url: String,
    duration: Option<u32>,
    rating: Option<String>,
    origin: Option<String>,
    aired_on: Option<PartialDate>,
    released_on: Option<PartialDate>,
    #[serde(default)]
    genres: Vec<GenreNode>,
}

#[derive(Deserialize)]
struct PartialDate {
    year: Option<i32>,
    month: Option<i32>,
    day: Option<i32>,
}

impl PartialDate {
    fn as_date(&self) -> Option<crate::animex::module::types::CalendarDate> {
        release_metadata::date(self.year?, self.month, self.day)
    }
}

#[derive(Deserialize)]
struct GenreNode {
    id: String,
    name: String,
    russian: String,
    kind: String,
}

fn parse_premiere(html: &str) -> Option<RegionalPremiere> {
    const MONTHS: [&str; 12] = [
        "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября",
        "октября", "ноября", "декабря",
    ];
    let document = Document::from(html);
    for node in document.select(".b-db_entry .c-info-left .line").nodes().iter().cloned() {
        let line = Selection::from(node);
        if line.select(".key").text().trim() != "Премьера в РФ:" {
            continue;
        }
        let value = line.select(".value").text();
        let mut parts = value.split_whitespace();
        let day = parts.next()?.parse().ok()?;
        let month = parts.next()?;
        let month = MONTHS.iter().position(|value| *value == month)? as i32 + 1;
        let year = parts.next()?.parse().ok()?;
        return Some(RegionalPremiere {
            country: "RU".into(), date: release_metadata::date(year, Some(month), Some(day))?,
        });
    }
    None
}

fn premiere(url: &str) -> Option<RegionalPremiere> {
    if !url.starts_with(&format!("{SITE}/animes/")) {
        return None;
    }
    let response = retry_rate_limited(|| {
        let response = anm_sdk::get(url)?;
        anm_sdk::ensure_success(&response, url)?;
        Ok(response)
    }).ok()?;
    parse_premiere(std::str::from_utf8(&response.body).ok()?)
}

pub(super) fn get(id: &str) -> Result<Release, String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("shikimori: invalid anime id".into());
    }
    let data: DetailsData = gql(QUERY, serde_json::json!({ "id": id }))?;
    let node = data.animes.into_iter().find(|node| node.anime.id == id)
        .ok_or_else(|| format!("shikimori: anime '{id}' not found"))?;
    let url = absolutize(&node.url);
    let mut details = map(node);
    details.regional_premieres = premiere(&url).into_iter().collect();
    Ok(details)
}

fn map(node: DetailsNode) -> Release {
    let mut details = release_metadata::upgrade(release(&node.anime));
    details.url = Some(absolutize(&node.url));
    details.aired_from = node.aired_on.as_ref().and_then(PartialDate::as_date);
    details.aired_to = node.released_on.as_ref().and_then(PartialDate::as_date);
    details.year = details.aired_from.as_ref().map(|date| date.year);
    details.episode_duration_seconds = node.duration.filter(|mins| *mins > 0).and_then(|mins| mins.checked_mul(60));
    details.age_rating = node.rating.as_deref().and_then(release_metadata::age_rating);
    details.original_source = node.origin.as_deref().and_then(release_metadata::original_source);
    details.rating = details.rating.filter(|rating| *rating > 0.0 && *rating <= 10.0);
    details.score_scale = details.rating.map(|_| 10);
    details.genres = node.genres.into_iter().filter(|genre| genre.kind == "genre")
        .map(|genre| Genre {
            url: Some(format!("{SITE}/animes/genre/{}-{}", genre.id, genre.name.replace(' ', "-"))),
            id: Some(genre.id),
            language: Some(if genre.russian.is_empty() { "en" } else { "ru" }.into()),
            name: if genre.russian.is_empty() { genre.name } else { genre.russian },
        }).collect();
    details
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_dates_and_metadata_from_the_details_response() {
        let raw = serde_json::json!({
            "id": "30911", "name": "Tales of Zestiria the Cross",
            "russian": "Сказания Зестирии", "english": "Tales of Zestiria the X",
            "url": "/animes/30911-tales-of-zestiria-the-cross",
            "score": 7.21, "rating": "pg_13", "duration": 23, "origin": "game",
            "airedOn": {"year": 2016, "month": 7, "day": 10},
            "genres": [{"id": "10", "name": "Fantasy", "russian": "Фэнтези", "kind": "genre"}]
        });
        let release = map(serde_json::from_value(raw).unwrap());
        assert_eq!(release.id, "30911");
        assert_eq!(release.aired_from.unwrap().day, Some(10));
        assert_eq!(release.episode_duration_seconds, Some(1380));
        assert_eq!(release.genres[0].id.as_deref(), Some("10"));
        assert_eq!(release.alternative_names[0].language, "en");
    }

    #[test]
    fn parses_only_the_russian_premiere_row() {
        let html = r#"<div class="b-db_entry"><div class="c-info-left"><div class="line">
            <div class="key">Премьера в РФ:</div><div class="value">10 июля 2016</div>
            </div></div></div>"#;
        let premiere = parse_premiere(html).unwrap();
        assert_eq!(premiere.country, "RU");
        assert_eq!(premiere.date.day, Some(10));
    }
}
