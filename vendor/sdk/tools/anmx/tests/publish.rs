use anm_repository::{RepositoryDescriptor, RepositoryIndex};
use std::{fs, process::Command};
#[test]
fn publisher_signs_packages_and_catalog_with_exact_manifest() {
    let temp = std::env::temp_dir().join(format!("anmx-publish-test-{}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();
    let manifest = serde_json::json!({"id":"org.test.source","kind":"source","name":"Test","version":"1.0.0","api_version":2,"author":{"name":"Tests","public_key":"placeholder"},"released_at":"2026-11-01T00:00:00Z","entry":"module.wasm","permissions":{"network":[],"storage":false},"dependencies":[{"id":"org.example.helper","requirement":"^1.2.0"}],"app_compatibility":{"min_app_version":">=0.1.0"}});
    fs::write(
        temp.join("module.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(temp.join("module.wasm"), b"test wasm bytes").unwrap();
    // Public, deterministic fixture key. Never used to sign real releases.
    let seed = anm_manifest::encode_ed25519(&[42; 32]);
    let pack = Command::new(env!("CARGO_BIN_EXE_anmx"))
        .env("ANMX_SIGNING_KEY", &seed)
        .args(["pack"])
        .arg(temp.join("module.json"))
        .arg(temp.join("module.wasm"))
        .arg(temp.join("test.anmx"))
        .output()
        .unwrap();
    assert!(
        pack.status.success(),
        "{}",
        String::from_utf8_lossy(&pack.stderr)
    );
    let publish = Command::new(env!("CARGO_BIN_EXE_anmx"))
        .env("ANMX_SIGNING_KEY", &seed)
        .args(["index", "org.test.repo", "Tests", "https://example.com/"])
        .arg(&temp)
        .arg(&temp)
        .output()
        .unwrap();
    assert!(
        publish.status.success(),
        "{}",
        String::from_utf8_lossy(&publish.stderr)
    );
    let repo: RepositoryDescriptor =
        serde_json::from_slice(&fs::read(temp.join("repo.json")).unwrap()).unwrap();
    let index: RepositoryIndex =
        serde_json::from_slice(&fs::read(temp.join("index.json")).unwrap()).unwrap();
    index.verify(&repo).unwrap();
    assert_eq!(index.modules.len(), 1);
    let entry = &index.modules[0];
    entry.manifest.verify(b"test wasm bytes").unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(temp.join("test.anmx")).unwrap()).unwrap();
    let actual: anm_manifest::Manifest =
        serde_json::from_reader(zip.by_name("module.json").unwrap()).unwrap();
    assert_eq!(actual, entry.manifest);
    assert_eq!(actual.dependencies.len(), 1);
    assert_eq!(actual.dependencies[0].id, "org.example.helper");
    assert!(actual.app_compatibility.is_some());
    fs::remove_dir_all(temp).unwrap();
}
