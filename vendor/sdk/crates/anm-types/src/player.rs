//! Player contract DTOs and IPC projections.

use serde::{Deserialize, Serialize};

use crate::module::ModuleMeta;

/// Candidate returned by `source-api.get-players`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerCandidate {
    pub protocol: String,
    pub payload: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerCapabilities {
    #[serde(default)]
    pub qualities: Vec<String>,
    #[serde(default)]
    pub protocols: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dub {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default)]
    pub qualities: Vec<String>,
    #[serde(default)]
    pub is_default: bool,
}

/// IPC projection of a resolved candidate. The opaque protocol payload stays
/// in the host session store; the frontend only receives `handle`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackCandidate {
    pub handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<ModuleMeta>,
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
    #[serde(default)]
    pub capabilities: PlayerCapabilities,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
