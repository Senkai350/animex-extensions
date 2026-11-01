//! Conformance player: implements `dummy.echo.v1` and echoes a URL from the
//! payload. Used by host integration tests, never bundled.

#[cfg(not(feature = "link-metadata"))]
wit_bindgen::generate!({
    path: "../../vendor/sdk/wit",
    world: "player-module",
});
#[cfg(feature = "link-metadata")]
wit_bindgen::generate!({ path: "../../vendor/sdk/wit", world: "player-module-link" });

use animex::module::types::{Dub, PlayerCapabilities, Quality, StreamData, StreamHls};
use exports::animex::module::player_api::Guest;
use serde::Deserialize;

const PROTOCOL: &str = "dummy.echo.v1";

struct Component;

#[derive(Deserialize)]
struct Payload {
    url: String,
    #[serde(default)]
    translation: Option<String>,
}

impl Guest for Component {
    fn describe() -> Result<PlayerCapabilities, String> {
        Ok(PlayerCapabilities {
            qualities: vec!["720p".into()],
            protocols: vec![PROTOCOL.into()],
            languages: vec!["xx".into()],
        })
    }

    fn get_dubs(payload: String) -> Result<Vec<Dub>, String> {
        let payload: Payload = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        Ok(vec![Dub {
            id: "default".into(),
            name: payload.translation.unwrap_or_else(|| "Dummy".into()),
            language: Some("xx".into()),
            qualities: vec!["720p".into()],
            is_default: true,
        }])
    }

    fn get_link(
        payload: String,
        _dub_id: String,
        _quality: Option<String>,
    ) -> Result<StreamData, String> {
        let payload: Payload = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        Ok(StreamData::Hls(StreamHls {
            url: payload.url.clone(),
            headers: Vec::new(),
            qualities: vec![Quality {
                label: "720p".into(),
                url: payload.url,
            }],
            subtitles: Vec::new(),
        }))
    }
}

export!(Component);

#[cfg(feature = "link-metadata")]
impl exports::animex::module::player_link_api::Guest for Component {
    fn get_link(payload: String, dub_id: String, quality: Option<String>) -> Result<exports::animex::module::player_link_api::PlaybackLink, String> {
        use animex::module::types::{TimeSkip, TimeSkipType};
        #[derive(Deserialize)]
        struct Skip { #[serde(rename = "type")] kind: String, start: f32, end: f32 }
        #[derive(Deserialize)]
        struct Metadata { #[serde(rename = "timeSkips")] time_skips: Option<Vec<Skip>> }
        let metadata: Metadata = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        let time_skips = metadata.time_skips.map(|items| items.into_iter().map(|item| Ok(TimeSkip {
            kind: match item.kind.as_str() { "opening" => TimeSkipType::Opening, "ending" => TimeSkipType::Ending, "other" => TimeSkipType::Other, _ => return Err("invalid skip type".to_string()) },
            start: item.start, end: item.end,
        })).collect::<Result<Vec<_>, String>>()).transpose()?;
        Ok(exports::animex::module::player_link_api::PlaybackLink {
            stream: <Self as Guest>::get_link(payload, dub_id, quality)?, time_skips,
        })
    }
}
