//! Library folders: exactly one folder per `(sourceId, releaseId)` title.

use serde::{Deserialize, Serialize};

/// Semantic role used by automatic folder transitions. The user can rename,
/// reorder or even delete role folders; roles are optional and unique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderRole {
    Planned,
    Watching,
    Completed,
    Dropped,
}

impl FolderRole {
    pub const ALL: [FolderRole; 4] = [
        FolderRole::Planned,
        FolderRole::Watching,
        FolderRole::Completed,
        FolderRole::Dropped,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FolderRole::Planned => "planned",
            FolderRole::Watching => "watching",
            FolderRole::Completed => "completed",
            FolderRole::Dropped => "dropped",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "planned" => Some(Self::Planned),
            "watching" => Some(Self::Watching),
            "completed" => Some(Self::Completed),
            "dropped" => Some(Self::Dropped),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<FolderRole>,
    pub position: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSummary {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<FolderRole>,
    pub position: i64,
    pub created_at: String,
    pub item_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub folder_id: String,
    pub source_id: String,
    pub release_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    pub added_at: String,
}
