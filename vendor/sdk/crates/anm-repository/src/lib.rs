//! Portable repository v1 contract shared by the host and publisher.
use anm_manifest::{canonical_json, decode_ed25519, encode_ed25519, Manifest};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_PACKAGE_SIZE: u64 = 64 * 1024 * 1024;
pub const MAX_INDEX_SIZE: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub index_url: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryIndex {
    pub schema_version: u32,
    pub repository_id: String,
    /// Monotonically increasing publication timestamp (seconds since Unix epoch).
    pub revision: u64,
    pub modules: Vec<RepositoryEntry>,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryEntry {
    pub manifest: Manifest,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

pub fn https_url(value: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(value).map_err(|e| e.to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Repository URLs must use HTTPS without credentials or fragments".into());
    }
    Ok(url)
}

pub fn public_key(value: &str) -> Result<VerifyingKey, String> {
    let bytes = decode_ed25519(value)?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "Expected a 32-byte Ed25519 key")?;
    VerifyingKey::from_bytes(&bytes).map_err(|e| e.to_string())
}

pub fn fingerprint(value: &str) -> Result<String, String> {
    Ok(hex::encode(Sha256::digest(public_key(value)?.as_bytes())))
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && !id.contains("..")
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

impl RepositoryDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION
            || !valid_id(&self.id)
            || self.name.trim().is_empty()
            || self.name.len() > 200
        {
            return Err("Invalid repository descriptor or unsupported schema".into());
        }
        https_url(&self.index_url)?;
        public_key(&self.public_key)?;
        Ok(())
    }
}

impl RepositoryIndex {
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut value = serde_json::to_value(self).expect("serializable index");
        value.as_object_mut().unwrap().remove("signature");
        // Domain separation prevents using a module signature as a catalog signature.
        let mut payload = b"AniMex repository index v1\0".to_vec();
        payload.extend(canonical_json(&value).as_bytes());
        payload
    }
    pub fn sign(&mut self, key: &SigningKey) {
        self.signature = encode_ed25519(&key.sign(&self.signing_payload()).to_bytes());
    }
    pub fn verify(&self, repo: &RepositoryDescriptor) -> Result<(), String> {
        repo.validate()?;
        if self.schema_version != SCHEMA_VERSION
            || self.repository_id != repo.id
            || self.modules.len() > 5000
        {
            return Err("Invalid index schema, repository identity or catalog size".into());
        }
        let signature =
            Signature::from_slice(&decode_ed25519(&self.signature)?).map_err(|e| e.to_string())?;
        public_key(&repo.public_key)?
            .verify_strict(&self.signing_payload(), &signature)
            .map_err(|e| e.to_string())?;
        let mut ids = HashSet::new();
        for entry in &self.modules {
            if !valid_id(&entry.manifest.id)
                || !ids.insert(&entry.manifest.id)
                || semver::Version::parse(&entry.manifest.version).is_err()
                || entry.size == 0
                || entry.size > MAX_PACKAGE_SIZE
                || entry.sha256.len() != 64
                || hex::decode(&entry.sha256).is_err()
            {
                return Err("Invalid or duplicate extension entry".into());
            }
            public_key(&entry.manifest.author.public_key)?;
            https_url(&entry.url)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (SigningKey, RepositoryDescriptor, RepositoryIndex) {
        let key = SigningKey::from_bytes(&[7; 32]);
        let repo = RepositoryDescriptor {
            schema_version: 1,
            id: "test.repo".into(),
            name: "Test".into(),
            index_url: "https://example.com/index.json".into(),
            public_key: encode_ed25519(key.verifying_key().as_bytes()),
        };
        let mut index = RepositoryIndex {
            schema_version: 1,
            repository_id: repo.id.clone(),
            revision: 1,
            modules: vec![],
            signature: String::new(),
        };
        index.sign(&key);
        (key, repo, index)
    }
    #[test]
    fn signatures_bind_identity_revision_and_empty_catalog() {
        let (_, repo, mut index) = fixture();
        index.verify(&repo).unwrap();
        index.revision += 1;
        assert!(index.verify(&repo).is_err());
        let (_, repo, mut index) = fixture();
        index.repository_id = "other".into();
        assert!(index.verify(&repo).is_err());
    }
    #[test]
    fn rejects_unsigned_and_wrong_keys() {
        let (_, mut repo, index) = fixture();
        repo.public_key =
            encode_ed25519(SigningKey::from_bytes(&[9; 32]).verifying_key().as_bytes());
        assert!(index.verify(&repo).is_err());
    }
    #[test]
    fn rejects_unsafe_urls_and_ids() {
        for url in [
            "http://example.com",
            "file:///test",
            "https://user:pass@example.com",
            "https://example.com/#token",
        ] {
            assert!(https_url(url).is_err());
        }
        for id in ["", "../test", "a/b", "a\\b"] {
            assert!(!valid_id(id));
        }
    }
}
