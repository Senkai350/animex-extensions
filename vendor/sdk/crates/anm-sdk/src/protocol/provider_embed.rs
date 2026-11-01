//! Source-independent payload for `cvh.embed.v1`, `kodik.embed.v1`, and
//! `aniboom.embed.v1`. One candidate per provider, one variant per dub.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedPayload {
    pub variants: Vec<EmbedVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedVariant {
    pub id: String,
    pub label: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cvh: Option<CvhContext>,
}

/// CVH integration settings belong to the source, never to a player default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CvhContext {
    pub media_id: String,
    pub publisher_id: String,
    pub aggregator: String,
    pub season: u32,
    pub episode: u32,
    pub voice: String,
}
