//! Release details, alternative titles and episodes.

use serde::{Deserialize, Serialize};

use crate::catalog::{AiringStatus, AnimeKind};

/// Stable release key plus an optional opaque source payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

/// Stable episode key plus an optional opaque source payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeRef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeSkipType {
    Opening,
    Ending,
    Other,
}

/// A skippable interval in an episode (opening, ending, ad, recap, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSkip {
    /// Serialized as `type` for the IPC contract.
    #[serde(rename = "type")]
    pub kind: TimeSkipType,
    pub start: f32,
    pub end: f32,
}

/// Title as presented by the source, with a language tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlternativeName {
    pub name: String,
    /// ISO 639-1 code (`ru`, `en`, `ja`, ...) or `unknown`.
    pub language: String,
}

/// Explicit tracker identifier supplied by a source or tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerData {
    /// Tracker module display name, e.g. `Shikimori`.
    pub name: String,
    /// Opaque tracker id string (numeric id, URL, slug).
    pub data: String,
}

/// Free-form title information rendered line by line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoBlock {
    pub title: String,
    pub data: Vec<String>,
}

/// Episode metadata only. Video links are resolved by a player module through
/// `playback_candidates` → `playback_dubs` → `playback_link`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: String,
    pub number: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
    #[serde(default)]
    pub time_skips: Vec<TimeSkip>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}

/// Release details returned by `source-api.get-release` and
/// `tracker-api.get-anime`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// Stable source key used by routing, history and library.
    pub id: String,
    /// Main title as the source presents it (AniLibria: Russian title).
    pub name: String,
    #[serde(default)]
    pub alternative_names: Vec<AlternativeName>,
    #[serde(default)]
    pub tracker_data: Vec<TrackerData>,
    #[serde(default)]
    pub other_info: Vec<InfoBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub studios: Vec<String>,
    #[serde(default)]
    pub status: AiringStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AnimeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_episodes: Option<u32>,
    /// RFC3339 UTC timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_episode_at: Option<String>,
    /// Rating relative to this source (not a third-party tracker).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<f32>,
    /// Opaque source payload for get-release / get-players.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}
