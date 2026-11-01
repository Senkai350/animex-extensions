//! Module identity, permissions and IPC metadata.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleKind {
    Source,
    Player,
    Tracker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
    pub public_key: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModulePermissions {
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub storage: bool,
}

/// IPC-представление установленного модуля.
///
/// `name`, `description` и `author` приходят из подписанного манифеста:
/// репозиторий не должен доверять данным, которые придумал сам WASM.
/// `qualities`, `protocols` и `languages` — тоже из манифеста, а `describe()`
/// player-модуля сверяется с ними при загрузке.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleMeta {
    pub id: String,
    pub kind: ModuleKind,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub author: ModuleAuthor,
    pub released_at: String,
    pub permissions: ModulePermissions,
    pub enabled: bool,
    pub trusted: bool,
    pub installed_at: String,
    pub path: String,
    #[serde(default)]
    pub built_in: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub qualities: Vec<String>,
    #[serde(default)]
    pub protocols: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}
