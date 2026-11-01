//! Stream payloads returned by player modules.

use serde::{Deserialize, Serialize};
use crate::TimeSkip;

/// Link response keeps legacy stream fields at the top level of IPC JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackLink {
    #[serde(flatten)]
    pub stream: StreamData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_skips: Option<Vec<TimeSkip>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrack {
    pub url: String,
    pub lang: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StreamData {
    Hls {
        url: String,
        #[serde(default)]
        headers: Vec<Header>,
        #[serde(default)]
        qualities: Vec<Quality>,
        #[serde(default)]
        subtitles: Vec<SubtitleTrack>,
    },
    Dash {
        url: String,
        #[serde(default)]
        headers: Vec<Header>,
        #[serde(default)]
        subtitles: Vec<SubtitleTrack>,
    },
    Mp4 {
        #[serde(default)]
        sources: Vec<Quality>,
        #[serde(default)]
        headers: Vec<Header>,
        #[serde(default)]
        subtitles: Vec<SubtitleTrack>,
    },
    Mpeg {
        url: String,
        #[serde(default)]
        headers: Vec<Header>,
    },
    Iframe {
        url: String,
    },
}

#[cfg(test)]
mod tests {
    use super::PlaybackLink;
    use serde_json::json;

    #[test]
    fn link_skips_are_optional_and_stream_shape_stays_flat() {
        for kind in ["hls", "dash", "mp4", "mpeg", "iframe"] {
            let legacy = json!({ "kind": kind, "url": "https://cdn.example/video" });
            let link: PlaybackLink = serde_json::from_value(legacy.clone()).unwrap();
            assert!(link.time_skips.is_none());
            assert!(serde_json::to_value(link).unwrap().get("timeSkips").is_none());
            for skips in [json!(null), json!([]), json!([{ "type": "opening", "start": 12.5, "end": 99.0 }])] {
                let mut value = legacy.clone();
                value["timeSkips"] = skips.clone();
                let link: PlaybackLink = serde_json::from_value(value).unwrap();
                let result = serde_json::to_value(link).unwrap();
                assert_eq!(result["kind"], kind);
                assert!(result.get("stream").is_none());
                if !skips.is_null() { assert_eq!(result["timeSkips"], skips); }
            }
        }
    }
}
