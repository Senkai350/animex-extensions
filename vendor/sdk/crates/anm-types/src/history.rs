//! Watch history DTO.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub source_id: String,
    pub release_id: String,
    pub episode_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    pub position_sec: u32,
    pub duration_sec: u32,
    pub updated_at: String,
}
