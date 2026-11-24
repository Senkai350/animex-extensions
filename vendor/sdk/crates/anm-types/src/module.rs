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
    /// Extension dependencies as declared in the manifest (id + requirement).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<ModuleDependency>,
    /// Host application version range this module supports, if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_compatibility: Option<ModuleAppCompatibility>,
}

/// IPC mirror of a manifest dependency entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDependency {
    pub id: String,
    /// npm-style semver requirement (`>=1.2.0`, `^2.0.0`, `*`, ...).
    pub requirement: String,
}

/// IPC mirror of the manifest host-application range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleAppCompatibility {
    pub min_app_version: Option<String>,
    pub max_app_version: Option<String>,
}
