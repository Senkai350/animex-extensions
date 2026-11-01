//! Manifest validation errors.

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing signature")]
    MissingSignature,
    #[error("bad public key: {0}")]
    BadPublicKey(String),
    #[error("bad signature: {0}")]
    BadSignature(String),
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("unsupported api version {0}")]
    UnsupportedApiVersion(u32),
    #[error("invalid version: {0}")]
    InvalidVersion(String),
    #[error("id must not be empty")]
    EmptyId,
    #[error("entry must not be empty")]
    EmptyEntry,
    #[error("invalid domain: {0}")]
    InvalidDomain(String),
    #[error("invalid field: {0}")]
    InvalidField(String),
}
