//! Manifest data model (snake_case on disk).

use serde::{Deserialize, Serialize};

use anm_types::ModuleKind;

/// Author in the on-disk manifest. Deliberately separate from the camelCase
/// `anm_types::ModuleAuthor` IPC mirror.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ManifestAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
    /// 32-byte Ed25519 public key as `ed25519:<base64>` (legacy hex accepted).
    pub public_key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ManifestPermissions {
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub storage: bool,
}

/// `module.json` from an `.anmx` package (plan §5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Manifest {
    pub id: String,
    pub kind: ModuleKind,
    pub name: String,
    /// semver (`semver::Version::parse` in `validate_basic`).
    pub version: String,
    pub api_version: u32,
    pub author: ManifestAuthor,
    pub released_at: String,
    pub entry: String,
    #[serde(default)]
    pub permissions: ManifestPermissions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Player capabilities, best-first. Rejected for sources.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qualities: Vec<String>,
    /// Protocol ids implemented (player) or emitted (source).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<String>,
    /// Language tags supported by the module.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    /// Extension dependencies (sources/trackers/players this module needs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<ManifestDependency>,
    /// Host application version range this module supports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_compatibility: Option<AppCompatibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// One dependency entry: `id` plus a semver requirement.
///
/// The requirement is the familiar npm-style string (`>=1.2.0`,
/// `^2.0.0`, `1.x`, `~1.4`, `>=1.0.0 <2.0.0`, `*`). Parsed by
/// [`crate::ranges::Requirement`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ManifestDependency {
    /// Extension id (`org.example.source`).
    pub id: String,
    /// Semver requirement range, npm-style.
    pub requirement: String,
}

/// Application compatibility range as two optional bounds.
///
/// `min_app_version` / `max_app_version` are semver requirement strings
/// applied to the host application version. Both optional; omitted means
/// unbounded on that side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppCompatibility {
    /// Minimum host version, e.g. `">=0.1.0"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
    /// Maximum host version, e.g. `"<2.0.0"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_app_version: Option<String>,
}
