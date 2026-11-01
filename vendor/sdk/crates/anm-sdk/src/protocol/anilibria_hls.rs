//! `anilibria.hls.v1`: direct HLS playlists exposed by AniLibria.
//!
//! The source publishes the available renditions of the single Aniliberty
//! voice-over. `player-hls` returns one dub and a `stream-data` value.

use serde::{Deserialize, Serialize};

/// Protocol id implemented by `player-hls` and emitted by `source-anilibria`.
pub const PROTOCOL: &str = "anilibria.hls.v1";

/// Static maximum qualities advertised by `player-hls`.
pub const QUALITIES: [&str; 3] = ["1080p", "720p", "480p"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityRef {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub episode_id: String,
    pub qualities: Vec<QualityRef>,
    /// Legacy field: older sources put individual cast credits here. Players
    /// ignore it; new sources leave it empty and omit it from serialized data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub translations: Vec<String>,
    #[serde(default)]
    pub referer: Option<String>,
}

impl Payload {
    /// Best available rendition URL.
    pub fn best_url(&self) -> Option<&str> {
        self.qualities.first().map(|q| q.url.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{Payload, QualityRef, PROTOCOL};

    #[test]
    fn protocol_id_is_valid_and_payload_roundtrips() {
        assert_eq!(PROTOCOL, "anilibria.hls.v1");
        let payload = Payload {
            episode_id: "e1".into(),
            qualities: vec![QualityRef {
                label: "1080p".into(),
                url: "https://cdn/x.m3u8".into(),
            }],
            translations: vec![],
            referer: Some("https://anilibria.top/".into()),
        };
        let text = serde_json::to_string(&payload).unwrap();
        let restored: Payload = serde_json::from_str(&text).unwrap();
        assert_eq!(restored.qualities[0].label, "1080p");
        assert_eq!(restored.best_url(), Some("https://cdn/x.m3u8"));
    }
}
