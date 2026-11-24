//! Manifest validation rules (plan §5.2).

use anm_types::ModuleKind;

use crate::error::ManifestError;
use crate::model::Manifest;
use crate::ranges::Requirement;

/// Maximum ABI version this host understands.
pub const CURRENT_API_VERSION: u32 = 2;
/// Old ABI versions are rejected: WIT 0.2 is a breaking contract change.
pub const MIN_SUPPORTED_API_VERSION: u32 = 2;

/// `protocol-id`: lowercase, 3..=64 chars, `[a-z0-9._-]`, starts alphanumeric.
pub fn valid_protocol_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !(3..=64).contains(&bytes.len()) {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
}

/// Quality labels are display strings (`1080p`, `4K`), not protocol ids.
pub fn valid_quality_label(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 32
        && trimmed == value
        && !value.chars().any(char::is_control)
}

fn validate_domain(domain: &str) -> Result<(), ManifestError> {
    let d = domain.trim();
    if d.is_empty() || d.contains('*') || d.starts_with('.') || !d.contains('.') {
        return Err(ManifestError::InvalidDomain(domain.to_string()));
    }
    Ok(())
}

fn validate_unique(values: &[String], field: &str) -> Result<(), ManifestError> {
    let mut seen = std::collections::HashSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ManifestError::InvalidField(format!(
                "{field} contains duplicate '{value}'"
            )));
        }
    }
    Ok(())
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        Ok(serde_json::from_slice(bytes)?)
    }

    /// Basic install-time validation (§5.2 items 1–5 plus ABI v2 fields).
    pub fn validate_basic(&self) -> Result<(), ManifestError> {
        if self.id.trim().is_empty() {
            return Err(ManifestError::EmptyId);
        }
        semver::Version::parse(&self.version)
            .map_err(|e| ManifestError::InvalidVersion(e.to_string()))?;
        if !(MIN_SUPPORTED_API_VERSION..=CURRENT_API_VERSION).contains(&self.api_version) {
            return Err(ManifestError::UnsupportedApiVersion(self.api_version));
        }
        if self.entry.trim().is_empty() {
            return Err(ManifestError::EmptyEntry);
        }
        for domain in &self.permissions.network {
            validate_domain(domain)?;
        }

        if self.kind == ModuleKind::Source && !self.qualities.is_empty() {
            return Err(ManifestError::InvalidField(
                "`qualities` is only valid for player modules".into(),
            ));
        }
        for quality in &self.qualities {
            if !valid_quality_label(quality) {
                return Err(ManifestError::InvalidField(format!(
                    "invalid quality label '{quality}'"
                )));
            }
        }
        validate_unique(&self.qualities, "qualities")?;

        for protocol in &self.protocols {
            if !valid_protocol_id(protocol) {
                return Err(ManifestError::InvalidField(format!(
                    "invalid protocol id '{protocol}'"
                )));
            }
        }
        validate_unique(&self.protocols, "protocols")?;

        if self.kind == ModuleKind::Player && self.protocols.is_empty() {
            return Err(ManifestError::InvalidField(
                "player modules must declare at least one protocol".into(),
            ));
        }

        for language in &self.languages {
            let trimmed = language.trim();
            if trimmed.is_empty() || trimmed.len() > 16 || trimmed != language {
                return Err(ManifestError::InvalidField(format!(
                    "invalid language tag '{language}'"
                )));
            }
        }
        validate_unique(&self.languages, "languages")?;

        // Dependencies: valid extension ids plus parseable semver ranges.
        let mut seen_deps = std::collections::HashSet::new();
        for dependency in &self.dependencies {
            if dependency.id.trim().is_empty() || dependency.id.len() > 160 {
                return Err(ManifestError::InvalidField(format!(
                    "invalid dependency id '{}' ",
                    dependency.id
                )));
            }
            if !seen_deps.insert(dependency.id.as_str()) {
                return Err(ManifestError::InvalidField(format!(
                    "duplicate dependency '{}' ",
                    dependency.id
                )));
            }
            Requirement::parse(&dependency.requirement).map_err(|e| {
                ManifestError::InvalidField(format!(
                    "dependency '{}': {e}",
                    dependency.id
                ))
            })?;
        }

        // Host application compatibility: both bounds must parse when set.
        if let Some(app) = &self.app_compatibility {
            if let Some(min) = &app.min_app_version {
                Requirement::parse(min).map_err(|e| {
                    ManifestError::InvalidField(format!("min_app_version: {e}"))
                })?;
            }
            if let Some(max) = &app.max_app_version {
                Requirement::parse(max).map_err(|e| {
                    ManifestError::InvalidField(format!("max_app_version: {e}"))
                })?;
            }
        }

        Ok(())
    }
}
