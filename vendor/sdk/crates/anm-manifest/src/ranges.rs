//! npm-style semver requirement ranges (dependencies, app compatibility).
//!
//! Supported: `=`, `>=`, `>`, `<`, `<=`, exact versions, `*`,
//! npm shortcuts `^x.y.z` / `~x.y.z`, partials `1.2` / `1` and
//! wildcards `1.x` / `1.2.*`; comparators combine with a space
//! (`">=1.0.0 <2.0.0"`). `||` unions are rejected.

use semver::{Version, VersionReq};

/// A validated requirement range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    raw: String,
    req: VersionReq,
}

impl Requirement {
    /// Parse and validate a requirement string.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("empty version requirement".into());
        }
        if trimmed.len() > 128 {
            return Err("version requirement too long".into());
        }
        if trimmed.contains("||") {
            return Err("'||' unions are not supported in version requirements".into());
        }
        if trimmed == "*" {
            return Ok(Self {
                raw: trimmed.to_string(),
                req: VersionReq::STAR,
            });
        }
        let expanded = Self::expand(trimmed)?;
        let req = VersionReq::parse(&expanded)
            .map_err(|e| format!("invalid requirement '{raw}': {e}"))?;
        if req.comparators.is_empty() {
            return Err(format!("requirement '{raw}' matches no version"));
        }
        Ok(Self {
            raw: trimmed.to_string(),
            req,
        })
    }

    /// The requirement as written in the manifest.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Does `version` satisfy this requirement?
    pub fn matches(&self, version: &Version) -> bool {
        if self.raw == "*" {
            return true;
        }
        self.req.matches(version)
    }

    /// Expand npm shortcuts into a plain `semver` range string.
    fn expand(raw: &str) -> Result<String, String> {
        let parts: Vec<&str> = raw.split_whitespace().collect();
        let mut out = String::new();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&Self::expand_one(part)?);
        }
        Ok(out)
    }

    fn expand_one(part: &str) -> Result<String, String> {
        let p = part.trim();
        if let Some(stripped) = p.strip_prefix('^') {
            let version = PartialVersion::parse(stripped, "^")?;
            return Ok(version.caret_range());
        }
        if let Some(stripped) = p.strip_prefix('~') {
            let version = PartialVersion::parse(stripped, "~")?;
            return Ok(format!(
                ">={version}, <{}.{}.0",
                version.major,
                version.minor.unwrap_or(0) + 1
            ));
        }
        if let Some((head, _)) = Self::split_wildcard(p) {
            let version = PartialVersion::parse(head, "")?;
            return Ok(version.partial_range());
        }
        for op in ["<=", ">=", "<", ">", "="] {
            if let Some(stripped) = p.strip_prefix(op) {
                let version = PartialVersion::parse(stripped, op)?;
                if version.patch.is_none() && !Self::split_wildcard(p).is_some() {
                    // Comparator + partial (`>=1.2`) still means a range floor,
                    // but semver requires a full version; keep the stated
                    // lower bound exact and do not add an implicit ceiling.
                    return Ok(format!("{op}{version}"));
                }
                return Ok(format!("{op}{version}"));
            }
        }
        let version = PartialVersion::parse(p, "")?;
        if version.patch.is_none() {
            return Ok(version.partial_range());
        }
        Ok(format!("={version}"))
    }

    fn split_wildcard(p: &str) -> Option<(&str, &str)> {
        for marker in [".x", ".*", ".X"] {
            if let Some(pos) = p.find(marker) {
                let head = &p[..pos];
                if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    return Some((head, &p[pos..]));
                }
            }
        }
        None
    }
}

/// A possibly-partial version (`1`, `1.2`, `1.2.3`).
#[derive(Debug, Clone)]
struct PartialVersion {
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
}

impl PartialVersion {
    fn parse(input: &str, prefix: &str) -> Result<Self, String> {
        let text = input.trim().trim_start_matches('v');
        if text.is_empty() {
            return Err(format!("missing version after '{prefix}'"));
        }
        let mut segments = text.split('.');
        let major: u64 = segments
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("invalid major version in '{input}'"))?;
        let minor: Option<u64> = segments.next().and_then(|s| s.parse().ok());
        let patch: Option<u64> = segments.next().and_then(|s| s.parse().ok());
        if segments.next().is_some() {
            return Err(format!("too many version segments in '{input}'"));
        }
        if minor.is_none() && patch.is_some() {
            return Err(format!("patch without minor in '{input}'"));
        }
        Ok(Self { major, minor, patch })
    }

    fn caret_range(&self) -> String {
        let upper = match (self.minor, self.patch) {
            (Some(minor), Some(patch)) => {
                if self.major > 0 {
                    format!("{}, <{}.0.0", self, self.major + 1)
                } else if minor > 0 {
                    format!("{}, <0.{}.0", self, minor + 1)
                } else {
                    format!("{}, <0.0.{}", self, patch + 1)
                }
            }
            (Some(minor), None) => {
                if self.major > 0 {
                    format!("{}, <{}.0.0", self, self.major + 1)
                } else {
                    format!("{}, <0.{}.0", self, minor + 1)
                }
            }
            (None, _) => format!("{}, <{}.0.0", self, self.major + 1),
        };
        format!(">={upper}")
    }

    fn partial_range(&self) -> String {
        let upper = match self.minor {
            Some(minor) => format!("{}, <{}.{}.0", self, self.major, minor + 1),
            None => format!("{}, <{}.0.0", self, self.major + 1),
        };
        format!(">={upper}")
    }
}

impl std::fmt::Display for PartialVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}.{}",
            self.major,
            self.minor.unwrap_or(0),
            self.patch.unwrap_or(0)
        )
    }
}
