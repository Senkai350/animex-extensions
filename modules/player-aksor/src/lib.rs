//! Aksor player module (`org.aksor.player`).
//!
//! Implements `aksor.embed.v1` for the Aksor iframe at
//! `https://player.aksor.tv/video/<32 lowercase hex>`. The exact URL is
//! validated (HTTPS, exact host, exact path shape, no credentials, no
//! non-default port) and returned as an iframe stream. Playback requests
//! are never cached and nothing is logged.

wit_bindgen::generate!({ path: "../../vendor/sdk/wit", world: "player-module" });
#[path = "../../player-embed-common/mod.rs"]
mod common;
use animex::module::types::{Dub, PlayerCapabilities, StreamData, StreamIframe};
use exports::animex::module::player_api::Guest;

const HOST: &str = "player.aksor.tv";

fn is_hex32(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn stream(url: &str) -> Result<StreamData, String> {
    let url = common::https_url(url)?;
    if url.host_str() != Some(HOST) {
        // Never echo the raw host or path in errors.
        return Err("aksor: unexpected host".into());
    }
    if url.port().is_some() {
        return Err("aksor: non-default ports are not allowed".into());
    }
    let segments: Vec<&str> = url
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    let valid = matches!(segments.as_slice(), ["video", id] if is_hex32(id));
    if !valid {
        return Err("aksor: expected /video/<32 lowercase hex>".into());
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
            protocols: vec!["aksor.embed.v1".into()],
            languages: vec!["ru".into()],
        })
    }
    fn get_dubs(payload: String) -> Result<Vec<Dub>, String> {
        common::dubs(&payload)
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

    const AKSOR_ID: &str = "0123456789abcdef0123456789abcdef";
    const PAYLOAD: &str = r#"{"referer":"https://old.yummyani.me/","variants":[
        {"id":"Озвучка AniLibria","label":"Озвучка AniLibria","url":"https://player.aksor.tv/video/0123456789abcdef0123456789abcdef"},
        {"id":"Субтитры","label":"Субтитры","language":"ru","url":"https://player.aksor.tv/video/fedcba9876543210fedcba9876543210"}]}"#;

    #[test]
    fn dubs_come_from_the_payload_in_order() {
        let dubs = Component::get_dubs(PAYLOAD.into()).unwrap();
        assert_eq!(dubs.len(), 2);
        assert_eq!(dubs[0].id, "Озвучка AniLibria");
        assert!(dubs[0].is_default);
        assert_eq!(dubs[1].language.as_deref(), Some("ru"));
        assert!(Component::get_dubs(r#"{"variants":[]}"#.into()).is_err());
    }

    #[test]
    fn link_returns_the_exact_video_path() {
        match Component::get_link(PAYLOAD.into(), "Субтитры".into(), None).unwrap() {
            StreamData::Iframe(value) => assert_eq!(
                value.url,
                "https://player.aksor.tv/video/fedcba9876543210fedcba9876543210"
            ),
            _ => panic!("expected iframe"),
        }
        assert!(Component::get_link(PAYLOAD.into(), "missing".into(), None).is_err());
        match stream(&format!("https://{HOST}/video/{AKSOR_ID}")).unwrap() {
            StreamData::Iframe(value) => {
                assert_eq!(value.url, format!("https://{HOST}/video/{AKSOR_ID}"))
            }
            _ => panic!("expected iframe"),
        }
    }

    #[test]
    fn rejects_foreign_hosts_wrong_paths_and_bad_ids() {
        for url in [
            "https://alloha.yani.tv/?token_movie=0&translation=1&season=1&episode=1&token=1",
            "https://kodikplayer.com/season/1/abc/720p",
            "https://player.aksor.tv/",
            "https://player.aksor.tv/video",
            // Too short, too long, non-hex and uppercase identifiers.
            "https://player.aksor.tv/video/0123456789abcdef",
            "https://player.aksor.tv/video/0123456789abcdef0123456789abcdeff",
            "https://player.aksor.tv/video/0123456789abcdef0123456789abcdeg",
            "https://player.aksor.tv/video/0123456789ABCDEF0123456789ABCDEF",
            // Extra path segments are not the exact video route.
            "https://player.aksor.tv/video/0123456789abcdef0123456789abcdef/extra",
            // Plain http, credentials and non-default ports are never accepted.
            "http://player.aksor.tv/video/0123456789abcdef0123456789abcdef",
            "https://user:pass@player.aksor.tv/video/0123456789abcdef0123456789abcdef",
            "https://player.aksor.tv:8443/video/0123456789abcdef0123456789abcdef",
        ] {
            assert!(stream(url).is_err(), "must reject {url}");
        }
    }

    #[test]
    fn errors_never_echo_the_raw_url() {
        let error = stream("https://secret-host.example/video/0123456789abcdef0123456789abcdef")
            .unwrap_err();
        assert!(!error.contains("secret-host.example"), "{error}");
    }
}
