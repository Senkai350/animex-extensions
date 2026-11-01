//! Ed25519 keys, signatures and the signing payload.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::error::ManifestError;
use crate::model::Manifest;
use crate::validate::CURRENT_API_VERSION;

/// Prefix for Ed25519 byte encoding: `ed25519:<base64>` (ADR-3).
pub const ED25519_PREFIX: &str = "ed25519:";

pub fn encode_ed25519(bytes: &[u8]) -> String {
    format!("{ED25519_PREFIX}{}", STANDARD.encode(bytes))
}

/// Decode `ed25519:<base64>`; legacy raw hex without a prefix is accepted.
pub fn decode_ed25519(value: &str) -> Result<Vec<u8>, String> {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix(ED25519_PREFIX) {
        STANDARD
            .decode(rest.trim())
            .map_err(|e| format!("base64 after `ed25519:`: {e}"))
    } else {
        hex::decode(value.trim()).map_err(|e| format!("legacy hex without `ed25519:` prefix: {e}"))
    }
}

impl Manifest {
    /// Bytes that are signed: `SHA256( SHA256(canonical_unsigned) || SHA256(wasm) )`.
    pub fn signing_payload(&self, wasm: &[u8]) -> [u8; 32] {
        let canonical = self.canonical_unsigned();
        let manifest_digest = Sha256::digest(canonical.as_bytes());
        let wasm_digest = Sha256::digest(wasm);

        let mut hasher = Sha256::new();
        hasher.update(manifest_digest);
        hasher.update(wasm_digest);
        hasher.finalize().into()
    }

    /// Sign the manifest; returns `ed25519:<base64>` for the `signature` field.
    pub fn sign_with(&self, signing_key: &ed25519_dalek::SigningKey, wasm: &[u8]) -> String {
        let payload = self.signing_payload(wasm);
        let signature: Signature = signing_key.sign(&payload);
        encode_ed25519(&signature.to_bytes())
    }

    /// Verify the module signature against `author.public_key`.
    pub fn verify(&self, wasm: &[u8]) -> Result<(), ManifestError> {
        if self.api_version > CURRENT_API_VERSION {
            return Err(ManifestError::UnsupportedApiVersion(self.api_version));
        }

        let sig_raw = self
            .signature
            .as_deref()
            .ok_or(ManifestError::MissingSignature)?;
        let sig_bytes =
            decode_ed25519(sig_raw).map_err(|e| ManifestError::BadSignature(e.to_string()))?;
        let signature = Signature::from_slice(&sig_bytes)
            .map_err(|e| ManifestError::BadSignature(e.to_string()))?;

        let pk_bytes = decode_ed25519(&self.author.public_key)
            .map_err(|e| ManifestError::BadPublicKey(e.to_string()))?;
        let pk_array: [u8; 32] = pk_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ManifestError::BadPublicKey("expected 32 bytes".into()))?;
        let verifying_key = VerifyingKey::from_bytes(&pk_array)
            .map_err(|e| ManifestError::BadPublicKey(e.to_string()))?;

        let payload = self.signing_payload(wasm);
        verifying_key
            .verify_strict(&payload, &signature)
            .map_err(|_| ManifestError::InvalidSignature)
    }

    /// Hex of the first 16 bytes of `SHA256(raw_public_key_bytes)`.
    pub fn key_fingerprint(&self) -> Option<String> {
        let raw = decode_ed25519(&self.author.public_key).ok()?;
        let digest = Sha256::digest(&raw);
        Some(hex::encode(&digest[..16]))
    }
}
