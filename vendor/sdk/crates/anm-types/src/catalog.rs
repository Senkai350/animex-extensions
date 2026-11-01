//! Catalog previews and pagination.

use serde::{Deserialize, Serialize};

use crate::details::{AlternativeName, TrackerData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiringStatus {
    Ongoing,
    Released,
    Anons,
    #[default]
    Unknown,
}

/// Canonical anime format values exposed to IPC. WIT keeps kind as an
/// optional string so module binaries can use legacy spellings; the host
/// normalizes them before they reach the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimeKind {
    Tv,
    Movie,
    Ova,
    Ona,
    Special,
    TvSpecial,
    Music,
    Pv,
    Cm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimePreview {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub alternative_names: Vec<AlternativeName>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AnimeKind>,
    #[serde(default)]
    pub status: AiringStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodes_count: Option<u32>,
    /// Tracker links using the same representation as `Release` (empty if unknown).
    #[serde(default)]
    pub tracker_data: Vec<TrackerData>,
    /// Opaque source payload round-tripped through `get_release`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMeta {
    pub page: u32,
    pub per_page: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<u32>,
    pub has_next: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    pub items: Vec<AnimePreview>,
    pub meta: PageMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogParams {
    pub page: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(default)]
    pub filters: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::{AnimeKind, AnimePreview, CatalogParams};

    #[test]
    fn preview_metadata_uses_release_names_and_optional_description() {
        let value = serde_json::json!({
            "id": "anime", "title": "Title",
            "alternativeNames": [{ "name": "別名", "language": "ja" }],
            "genres": ["Fantasy"], "description": "Synopsis"
        });
        let preview: AnimePreview = serde_json::from_value(value.clone()).unwrap();
        let serialized = serde_json::to_value(preview).unwrap();
        for field in ["alternativeNames", "genres", "description"] {
            assert_eq!(serialized[field], value[field]);
        }
        assert!(serialized.get("titleOriginal").is_none());

        let minimal: AnimePreview = serde_json::from_value(serde_json::json!({
            "id": "anime", "title": "Title"
        }))
        .unwrap();
        let serialized = serde_json::to_value(minimal).unwrap();
        assert_eq!(serialized["alternativeNames"], serde_json::json!([]));
        assert_eq!(serialized["genres"], serde_json::json!([]));
        assert!(serialized.get("description").is_none());
    }

    #[test]
    fn preview_serializes_tracker_data_for_arbitrary_trackers_and_empty_lists() {
        for trackers in [
            serde_json::json!([]),
            serde_json::json!([
                { "name": "Shikimori", "data": "20" },
                { "name": "Custom Tracker", "data": "opaque-id" }
            ]),
        ] {
            let value = serde_json::json!({
                "id": "anime", "title": "Title", "status": "unknown",
                "trackerData": trackers
            });
            let preview: AnimePreview = serde_json::from_value(value).unwrap();
            let serialized = serde_json::to_value(preview).unwrap();
            assert_eq!(serialized["trackerData"], trackers);
            assert!(serialized.get("externalIds").is_none());
        }
    }

    #[test]
    fn catalog_params_forward_the_chosen_sort_json() {
        let params = CatalogParams {
            page: 1,
            search: None,
            sort: Some(r#"{"value":"title","direction":"asc"}"#.into()),
            filters: vec![],
        };
        let json = serde_json::to_value(params).expect("serialize catalog params");
        assert_eq!(json["sort"], r#"{"value":"title","direction":"asc"}"#);
    }

    #[test]
    fn anime_kind_uses_canonical_snake_case_values() {
        assert_eq!(
            serde_json::to_string(&AnimeKind::TvSpecial).unwrap(),
            "\"tv_special\""
        );
    }
}
