//! Alloha player module (`org.alloha.player`).
//!
//! Implements `alloha.embed.v1` for the official Alloha iframe at
//! `https://alloha.yani.tv/` with the `token_movie`, `translation`,
//! `season`, `episode` and `token` parameters. The exact URL is validated
//! (HTTPS, exact host, exact `/` path, no credentials, no non-default
//! port; every required parameter present exactly once and never blank)
//! and returned as an iframe stream. Parameter values stay opaque: they
//! are never inspected, printed, cached or logged.

wit_bindgen::generate!({ path: "../../vendor/sdk/wit", world: "player-module" });
#[path = "../../player-embed-common/mod.rs"]
mod common;
use animex::module::types::{Dub, PlayerCapabilities, StreamData, StreamIframe};
use exports::animex::module::player_api::Guest;

const HOST: &str = "alloha.yani.tv";
const REQUIRED_PARAMS: &[&str] = &["token_movie", "translation", "season", "episode", "token"];

fn stream(url: &str) -> Result<StreamData, String> {
    let url = common::https_url(url)?;
    if url.host_str() != Some(HOST) {
        // Never echo the raw host or path in errors.
        return Err("alloha: unexpected host".into());
    }
    if url.port().is_some() {
        return Err("alloha: non-default ports are not allowed".into());
    }
    if url.path() != "/" {
        return Err("alloha: unexpected path".into());
    }
    let mut present: Vec<String> = Vec::new();
    for (name, value) in url.query_pairs() {
        let name = name.into_owned();
        if !REQUIRED_PARAMS.contains(&name.as_str()) {
            continue;
        }
        // Required parameters appear exactly once and are never blank.
        if present.contains(&name) || value.trim().is_empty() {
            return Err("alloha: malformed required parameter".into());
        }
        present.push(name);
    }
    for required in REQUIRED_PARAMS {
        if !present.iter().any(|name| name == required) {
            return Err(format!("alloha: missing '{required}' parameter"));
        }
    }
    Ok(StreamData::Iframe(StreamIframe {
        url: url.to_string(),
    }))
}

struct Component;
impl Guest for Component {
    fn describe() -> Result<PlayerCapabilities, String> {
        Ok(PlayerCapabilities {
            qualities: vec![],
            protocols: vec!["alloha.embed.v1".into()],
            languages: vec!["ru".into()],
        })
    }
    fn get_dubs(payload: String) -> Result<Vec<Dub>, String> {
        let payload = common::parse(&payload)?;
        Ok(payload
            .variants
            .into_iter()
            .enumerate()
            .map(|(index, variant)| Dub {
                id: variant.id,
                name: variant.label,
                language: variant.language,
                qualities: vec![],
                is_default: index == 0,
            })
            .collect())
    }
    fn get_link(
        payload: String,
        dub_id: String,
        _quality: Option<String>,
    ) -> Result<StreamData, String> {
        let payload = common::parse(&payload)?;
        let variant = common::selected(&payload, &dub_id)?;
        stream(&variant.url)
    }
}
export!(Component);
#[cfg(test)]
mod tests {
    use super::*;

    /// Placeholder tokens of the real shape; live values never appear here.
    const PAYLOAD: &str = r#"{"referer":"https://old.yummyani.me/","variants":[
        {"id":"Озвучка AniLibria","label":"Озвучка AniLibria","url":"https://alloha.yani.tv/?token_movie=00000000000000000000000000000000&translation=340&season=1&episode=1&token=111111111111111111111111111111&hidden=translation,season,episode"},
        {"id":"Субтитры","label":"Субтитры","language":"ru","url":"https://alloha.yani.tv/?token_movie=00000000000000000000000000000000&translation=341&season=1&episode=2&token=111111111111111111111111111111&hidden=translation,season,episode"}]}"#;

    #[test]
    fn dubs_come_from_the_payload_in_order() {
        let dubs = Component::get_dubs(PAYLOAD.into()).unwrap();
        assert_eq!(dubs.len(), 2);
        assert_eq!(dubs[0].id, "Озвучка AniLibria");
        assert!(dubs[0].is_default);
        assert!(!dubs[1].is_default);
        assert_eq!(dubs[1].language.as_deref(), Some("ru"));
    }

    #[test]
    fn link_returns_the_official_iframe_exactly() {
        match Component::get_link(PAYLOAD.into(), "Субтитры".into(), None).unwrap() {
            StreamData::Iframe(value) => assert_eq!(
                value.url,
                "https://alloha.yani.tv/?token_movie=00000000000000000000000000000000&translation=341&season=1&episode=2&token=111111111111111111111111111111&hidden=translation,season,episode"
            ),
            _ => panic!("expected iframe"),
        }
        assert!(Component::get_link(PAYLOAD.into(), "missing".into(), None).is_err());
    }

    #[test]
    fn rejects_foreign_hosts_paths_schemes_and_incomplete_urls() {
        for url in [
            "https://player.aksor.tv/video/0123456789abcdef0123456789abcdef",
            "https://alloha.yani.tv/video/1",
            "https://alloha.yani.tv/embed",
            // No query parameters at all.
            "https://alloha.yani.tv/",
            // Missing `token_movie`.
            "https://alloha.yani.tv/?translation=1&season=1&episode=1&token=1",
            // Blank required parameter.
            "https://alloha.yani.tv/?token_movie=&translation=1&season=1&episode=1&token=1",
            // Duplicate required parameter.
            "https://alloha.yani.tv/?token_movie=0&translation=1&season=1&episode=1&token=1&token=2",
            // Plain http and credentials are never accepted.
            "http://alloha.yani.tv/?token_movie=0&translation=1&season=1&episode=1&token=1",
            "https://user:pass@alloha.yani.tv/?token_movie=0&translation=1&season=1&episode=1&token=1",
            // Non-default ports are never accepted.
            "https://alloha.yani.tv:8443/?token_movie=0&translation=1&season=1&episode=1&token=1",
        ] {
            assert!(stream(url).is_err(), "must reject {url}");
        }
    }

    #[test]
    fn errors_never_echo_the_raw_url() {
        let error = stream("https://secret-host.example/video/path").unwrap_err();
        assert!(!error.contains("secret-host.example"), "{error}");
        assert!(!error.contains("path"), "{error}");
    }
}
