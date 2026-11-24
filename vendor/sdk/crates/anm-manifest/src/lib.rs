//! Manifest parsing, canonicalization, Ed25519 verification and IPC mapping.
//!
//! On-disk `module.json` is snake_case (ADR-2). `anm_types::ModuleMeta` is the
//! camelCase IPC mirror. The signature covers
//! `ed25519(sha256(sha256(canonical(manifest_without_signature)) || sha256(module.wasm)))`.

mod canonical;
mod error;
mod meta;
mod model;
mod ranges;
mod signature;
mod validate;

pub use canonical::canonical_json;
pub use error::ManifestError;
pub use model::{AppCompatibility, Manifest, ManifestAuthor, ManifestDependency, ManifestPermissions};
pub use ranges::Requirement;
pub use signature::{decode_ed25519, encode_ed25519, ED25519_PREFIX};
pub use validate::{
    valid_protocol_id, valid_quality_label, CURRENT_API_VERSION, MIN_SUPPORTED_API_VERSION,
};

#[cfg(test)]
mod tests;
