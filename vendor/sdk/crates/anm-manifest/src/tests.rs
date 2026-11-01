//! Manifest unit tests.

use ed25519_dalek::SigningKey;

use crate::{
    encode_ed25519, valid_protocol_id, valid_quality_label, Manifest, ManifestAuthor,
    ManifestError, ManifestPermissions, CURRENT_API_VERSION,
};

const SAMPLE_JSON: &str = r#"{
    "id": "org.anilibria.source",
    "kind": "source",
    "name": "AniLibria",
    "version": "1.0.0",
    "api_version": 2,
    "author": {
        "name": "AniMex",
        "contact": "https://github.com/example",
        "public_key": "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    },
    "released_at": "2026-10-20T00:00:00Z",
    "entry": "module.wasm",
    "permissions": {
        "network": ["anilibria.top", "cache.libria.fun"],
        "storage": true
    },
    "protocols": ["anilibria.hls.v1"],
    "languages": ["ru"],
    "description": "Официальный источник AniLibria"
}"#;

fn sample() -> Manifest {
    Manifest::parse(SAMPLE_JSON.as_bytes()).expect("sample parses")
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn key_public_b64(sk: &SigningKey) -> String {
    encode_ed25519(&sk.verifying_key().to_bytes())
}

#[test]
fn parses_snake_case_manifest_and_v2_fields() {
    let m = sample();
    assert_eq!(m.id, "org.anilibria.source");
    assert_eq!(m.api_version, 2);
    assert_eq!(m.protocols, vec!["anilibria.hls.v1"]);
    assert_eq!(m.languages, vec!["ru"]);
    assert!(m.qualities.is_empty());
    assert_eq!(m.signature, None);

    let json = serde_json::to_string(&m).expect("serialize");
    assert!(json.contains("\"api_version\":2"), "{json}");
    assert!(json.contains("\"released_at\""), "{json}");
    assert!(!json.contains("apiVersion"), "{json}");
}

#[test]
fn canonical_is_stable_across_key_order() {
    let other = r#"{
        "permissions": {"storage": true, "network": ["anilibria.top", "cache.libria.fun"]},
        "author": {"public_key": "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=", "contact": "https://github.com/example", "name": "AniMex"},
        "description": "Официальный источник AniLibria",
        "released_at": "2026-10-20T00:00:00Z",
        "entry": "module.wasm",
        "api_version": 2,
        "version": "1.0.0",
        "name": "AniLibria",
        "kind": "source",
        "protocols": ["anilibria.hls.v1"],
        "languages": ["ru"],
        "id": "org.anilibria.source"
    }"#;
    let a = Manifest::parse(other.as_bytes()).expect("other parses");
    assert_eq!(a, sample());
    assert_eq!(a.canonical_unsigned(), sample().canonical_unsigned());

    let canonical = sample().canonical_unsigned();
    assert!(!canonical.contains("\": \""), "{canonical}");
    assert!(!canonical.contains('\n'), "{canonical}");
    assert!(canonical.starts_with("{\"api_version\":2"), "{canonical}");
    assert!(!canonical.contains("signature"), "{canonical}");
}

#[test]
fn sign_and_verify_roundtrip() {
    let mut m = sample();
    let wasm = b"\0asm\x01\x00\x00\x00 fake wasm";
    let sk = signing_key();
    m.author.public_key = key_public_b64(&sk);
    m.signature = Some(m.sign_with(&sk, wasm));
    m.verify(wasm).expect("valid signature");
    assert!(m.verify(b"different wasm").is_err());
}

#[test]
fn verify_accepts_legacy_hex() {
    let mut m = sample();
    let sk = signing_key();
    let wasm = b"wasm bytes";
    m.author.public_key = hex::encode(sk.verifying_key().to_bytes());
    let payload = m.signing_payload(wasm);
    let sig = ed25519_dalek::Signer::sign(&sk, &payload);
    m.signature = Some(hex::encode(sig.to_bytes()));
    m.verify(wasm).expect("legacy hex must verify");
}

#[test]
fn rejects_old_and_future_api_versions() {
    for version in [1, CURRENT_API_VERSION + 1] {
        let mut m = sample();
        m.api_version = version;
        assert!(
            matches!(m.validate_basic(), Err(ManifestError::UnsupportedApiVersion(v)) if v == version)
        );
    }
}

#[test]
fn player_capabilities_are_validated() {
    let mut player = sample();
    player.kind = anm_types::ModuleKind::Player;
    player.qualities = vec!["1080p".into(), "720p".into()];
    player.protocols = vec!["anilibria.hls.v1".into()];
    player.validate_basic().expect("valid player");

    let mut duplicate = player.clone();
    duplicate.qualities = vec!["1080p".into(), "1080p".into()];
    assert!(matches!(
        duplicate.validate_basic(),
        Err(ManifestError::InvalidField(_))
    ));

    let mut source_with_quality = sample();
    source_with_quality.qualities = vec!["1080p".into()];
    assert!(matches!(
        source_with_quality.validate_basic(),
        Err(ManifestError::InvalidField(_))
    ));

    let mut player_without_protocol = player.clone();
    player_without_protocol.protocols.clear();
    assert!(matches!(
        player_without_protocol.validate_basic(),
        Err(ManifestError::InvalidField(_))
    ));

    let mut bad_protocol = player;
    bad_protocol.protocols = vec!["Not A Protocol".into()];
    assert!(matches!(
        bad_protocol.validate_basic(),
        Err(ManifestError::InvalidField(_))
    ));
}

#[test]
fn protocol_and_quality_validators_are_strict() {
    assert!(valid_protocol_id("anilibria.hls.v1"));
    assert!(valid_protocol_id("kodik-embed_v2"));
    for bad in ["", "X", "Протокол", ".hidden", "a b", "A.B"] {
        assert!(!valid_protocol_id(bad), "{bad}");
    }
    assert!(valid_quality_label("1080p"));
    assert!(!valid_quality_label(" 1080p"));
    assert!(!valid_quality_label(""));
}

#[test]
fn validate_basic_checks_fields_and_domains() {
    sample().validate_basic().expect("sample is valid");

    let mut m = sample();
    m.version = "1.0".into();
    assert!(matches!(
        m.validate_basic(),
        Err(ManifestError::InvalidVersion(_))
    ));

    let mut m = sample();
    m.entry = "  ".into();
    assert!(matches!(m.validate_basic(), Err(ManifestError::EmptyEntry)));

    let mut m = sample();
    m.id = "".into();
    assert!(matches!(m.validate_basic(), Err(ManifestError::EmptyId)));

    for bad in ["", "*", "*.anilibria.top", ".anilibria.top", "localhost"] {
        let mut m = sample();
        m.permissions.network = vec![bad.into()];
        assert!(
            matches!(m.validate_basic(), Err(ManifestError::InvalidDomain(_))),
            "domain `{bad}` must be rejected"
        );
    }
}

#[test]
fn parse_rejects_unknown_kind() {
    let bad = SAMPLE_JSON.replace("\"kind\": \"source\"", "\"kind\": \"wizard\"");
    assert!(Manifest::parse(bad.as_bytes()).is_err());
}

#[test]
fn key_fingerprint_is_sha256_of_raw_bytes() {
    use sha2::{Digest, Sha256};
    let mut m = sample();
    let sk = signing_key();
    let raw = sk.verifying_key().to_bytes();
    m.author.public_key = encode_ed25519(&raw);
    let expected = {
        let digest = Sha256::digest(raw);
        hex::encode(&digest[..16])
    };
    assert_eq!(m.key_fingerprint().as_deref(), Some(expected.as_str()));
    m.author.public_key = hex::encode(raw);
    assert_eq!(m.key_fingerprint().as_deref(), Some(expected.as_str()));
}

#[test]
fn to_meta_maps_new_fields_and_camelcase() {
    let mut m = sample();
    m.kind = anm_types::ModuleKind::Player;
    m.qualities = vec!["1080p".into(), "720p".into()];
    m.protocols = vec!["anilibria.hls.v1".into()];
    let meta = m.to_meta(true, false, "2026-10-20T12:00:00Z", "C:/modules/x");
    assert_eq!(meta.qualities, vec!["1080p", "720p"]);
    assert_eq!(meta.protocols, vec!["anilibria.hls.v1"]);
    assert_eq!(meta.languages, vec!["ru"]);
    let json = serde_json::to_string(&meta).expect("meta serializes");
    assert!(json.contains("\"apiVersion\":2"), "{json}");
    assert!(json.contains("\"qualities\""), "{json}");
}

#[test]
fn manifest_author_and_permissions_are_exported_for_tooling() {
    let author = ManifestAuthor {
        name: "AniMex".into(),
        contact: None,
        public_key: "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
    };
    let permissions = ManifestPermissions::default();
    assert!(author.public_key.starts_with("ed25519:"));
    assert!(!permissions.storage);
}
