//! Tracker DTOs (Shikimori and future trackers).

use serde::{Deserialize, Serialize};

use crate::details::AlternativeName;

/// Tracker metadata, independent of source episode IDs and playback payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerEpisode {
    pub id: String,
    pub number: f64,
    pub title: Option<String>,
    pub alternative_names: Vec<AlternativeName>,
    pub aired_at: Option<String>,
    pub rating: Option<f32>,
    pub score_scale: u8,
    pub is_filler: Option<bool>,
    pub is_recap: Option<bool>,
    pub url: Option<String>,
    pub forum_url: Option<String>,
    pub video_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerEpisodePage {
    pub items: Vec<TrackerEpisode>,
    pub meta: crate::PageMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffRef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub alternative_names: Vec<AlternativeName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub alternative_names: Vec<AlternativeName>,
    #[serde(default)]
    pub tracker_data: Vec<crate::TrackerData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default)]
    pub seiyuu: Vec<StaffRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffMember {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub alternative_names: Vec<AlternativeName>,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoKind {
    Opening,
    Ending,
    Pv,
    Trailer,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerVideo {
    pub id: String,
    pub kind: VideoKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub original: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Anime,
    Manga,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleCard {
    pub id: String,
    pub name: String,
    pub alternative_names: Vec<AlternativeName>,
    pub poster: Option<String>,
    pub media_type: MediaType,
    pub format: Option<String>,
    pub status: crate::AiringStatus,
    pub year: Option<u16>,
    pub tracker_data: Vec<crate::TrackerData>,
    pub source_ref: Option<String>,
    pub url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleRelation {
    pub code: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedTitle {
    pub title: TitleCard,
    pub relation: TitleRelation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarTitle {
    pub title: TitleCard,
    pub reason: Option<String>,
}
/// Host-owned context. None means unsupported; Some with [] means no results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleFeed<T> {
    pub module_id: String,
    pub items: Vec<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreCount {
    pub score: u8,
    pub count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCount {
    pub status: String,
    pub label: String,
    pub count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleStatistics {
    pub rating: Option<f32>,
    pub score_scale: u8,
    pub scores: Vec<ScoreCount>,
    pub statuses: Vec<StatusCount>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsFeed {
    pub module_id: String,
    pub statistics: TitleStatistics,
}

#[cfg(test)]
mod statistics_tests {
    use super::*;
    #[test]
    fn episode_metadata_roundtrips_nulls_false_flags_and_date_time() {
        let json = serde_json::json!({
            "id": "1", "number": 1.5, "title": null, "alternativeNames": [],
            "airedAt": "2026-10-29T00:00:00", "rating": null, "scoreScale": 5,
            "isFiller": false, "isRecap": null, "url": null, "forumUrl": null, "videoUrl": null
        });
        let episode: TrackerEpisode = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(episode).unwrap(), json);
    }
    #[test]
    fn character_tracker_links_default_to_empty_and_roundtrip_as_camel_case() {
        let minimal: Character =
            serde_json::from_value(serde_json::json!({ "id": "42", "name": "Character" })).unwrap();
        assert!(minimal.tracker_data.is_empty());
        let mut character = minimal;
        character.tracker_data.push(crate::TrackerData {
            name: "Shikimori".into(),
            data: "42".into(),
        });
        let json = serde_json::to_value(character).unwrap();
        assert_eq!(json["trackerData"][0]["name"], "Shikimori");
        assert_eq!(json["trackerData"][0]["data"], "42");
    }
    #[test]
    fn statistics_feed_uses_camel_case_without_changing_release() {
        let feed = StatisticsFeed {
            module_id: "tracker".into(),
            statistics: TitleStatistics {
                rating: None,
                score_scale: 10,
                scores: vec![],
                statuses: vec![],
            },
        };
        let json = serde_json::to_value(feed).unwrap();
        assert_eq!(json["moduleId"], "tracker");
        assert_eq!(json["statistics"]["scoreScale"], 10);
        assert_eq!(json["statistics"]["rating"], serde_json::Value::Null);
        assert_eq!(json["statistics"]["scores"], serde_json::json!([]));
        assert_eq!(json["statistics"]["statuses"], serde_json::json!([]));
    }
}
