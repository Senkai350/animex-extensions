//! Package/sign a prepared module and publish a repository index.
use anm_manifest::{decode_ed25519, encode_ed25519, Manifest};
use anm_repository::{RepositoryDescriptor, RepositoryEntry, RepositoryIndex};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::{env, fs, io::Write, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let seed = env::var("ANMX_SIGNING_KEY")
        .map_err(|_| "Set ANMX_SIGNING_KEY to an ed25519:base64 32-byte private seed")?;
    let seed: [u8; 32] = decode_ed25519(&seed)?
        .try_into()
        .map_err(|_| "Expected 32-byte private seed")?;
    let key = SigningKey::from_bytes(&seed);
    match args.first().map(String::as_str) {
        Some("public-key") => println!("{}", encode_ed25519(key.verifying_key().as_bytes())),
        Some("pack") if args.len() == 4 => {
            let mut manifest: Manifest = serde_json::from_slice(&fs::read(&args[1])?)?;
            let wasm = fs::read(&args[2])?;
            manifest.author.public_key = encode_ed25519(key.verifying_key().as_bytes());
            manifest.validate_basic()?;
            manifest.signature = Some(manifest.sign_with(&key, &wasm));
            manifest.verify(&wasm)?;
            let mut zip = zip::ZipWriter::new(fs::File::create(&args[3])?);
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("module.json", options)?;
            zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
            zip.start_file("module.wasm", options)?;
            zip.write_all(&wasm)?;
            zip.finish()?;
        }
        Some("index") if args.len() == 6 => {
            // index <id> <name> <public base URL ending in /> <package directory> <output directory>
            let base = anm_repository::https_url(&args[3])?;
            if !base.path().ends_with('/') { return Err("Base URL must end with /".into()); }
            let out = Path::new(&args[5]); fs::create_dir_all(out)?;
            let repo = RepositoryDescriptor { schema_version: 1, id: args[1].clone(), name: args[2].clone(), index_url: base.join("index.json")?.to_string(), public_key: encode_ed25519(key.verifying_key().as_bytes()) };
            let mut modules = vec![];
            for entry in fs::read_dir(&args[4])? {
                let path = entry?.path();
                if path.extension().and_then(|v|v.to_str()) != Some("anmx") { continue; }
                let bytes = fs::read(&path)?;
                let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes))?;
                let manifest: Manifest = serde_json::from_reader(zip.by_name("module.json")?)?;
                let mut wasm = Vec::new();
                std::io::Read::read_to_end(&mut zip.by_name(&manifest.entry)?, &mut wasm)?;
                manifest.verify(&wasm)?;
                modules.push(RepositoryEntry { manifest, url: base.join(path.file_name().unwrap().to_str().ok_or("Invalid filename")?)?.to_string(), sha256: hex::encode(Sha256::digest(&bytes)), size: bytes.len() as u64 });
            }
            modules.sort_by(|a,b|a.manifest.id.cmp(&b.manifest.id));
            let mut index = RepositoryIndex { schema_version: 1, repository_id: repo.id.clone(), revision: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(), modules, signature: String::new() };
            index.sign(&key); index.verify(&repo)?;
            fs::write(out.join("repo.json"), serde_json::to_vec_pretty(&repo)?)?;
            fs::write(out.join("index.json"), serde_json::to_vec_pretty(&index)?)?;
        }
        _ => return Err("Usage: anmx public-key | pack <module.json> <module.wasm> <output.anmx> | index <id> <name> <base-url/> <packages> <output>".into()),
    }
    Ok(())
}
