//! First-party player for `anilibria.hls.v1`.
//!
//! AniLibria exposes one Aniliberty voice-over with a direct HLS URL per
//! quality. Release members are credits, not separate audio tracks.

wit_bindgen::generate!({
    path: "../../vendor/sdk/wit",
    world: "player-module",
});

use animex::module::types::{Dub, Header, PlayerCapabilities, Quality, StreamData, StreamHls};
use anm_sdk::protocol::anilibria_hls;
use exports::animex::module::player_api::Guest;

struct Component;

impl Guest for Component {
    fn describe() -> Result<PlayerCapabilities, String> {
        Ok(PlayerCapabilities {
            qualities: anilibria_hls::QUALITIES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            protocols: vec![anilibria_hls::PROTOCOL.to_string()],
            languages: vec!["ru".to_string()],
        })
    }

    fn get_dubs(payload: String) -> Result<Vec<Dub>, String> {
        let payload: anilibria_hls::Payload = anm_sdk::protocol::parse_payload(&payload)?;
        Ok(dubs(&payload))
    }

    fn get_link(
        payload: String,
        dub_id: String,
        quality: Option<String>,
    ) -> Result<StreamData, String> {
        let payload: anilibria_hls::Payload = anm_sdk::protocol::parse_payload(&payload)?;
        let index = dub_id
            .parse::<usize>()
            .map_err(|_| format!("unknown dub '{dub_id}'"))?;
        if index != 0 {
            return Err(format!("unknown dub '{dub_id}'"));
        }
        let selected = select_quality(&payload, quality.as_deref())?;
        let headers = payload
            .referer
            .as_ref()
            .map(|referer| {
                vec![Header {
                    name: "Referer".to_string(),
                    value: referer.clone(),
                }]
            })
            .unwrap_or_default();
        Ok(StreamData::Hls(StreamHls {
            url: selected.url.clone(),
            headers,
            qualities: payload
                .qualities
                .iter()
                .map(|quality| Quality {
                    label: quality.label.clone(),
                    url: quality.url.clone(),
                })
                .collect(),
            subtitles: Vec::new(),
        }))
    }
}

fn dubs(payload: &anilibria_hls::Payload) -> Vec<Dub> {
    let qualities = payload
        .qualities
        .iter()
        .map(|quality| quality.label.clone())
        .collect::<Vec<_>>();
    vec![Dub {
        id: "0".to_string(),
        name: "Aniliberty".to_string(),
        language: Some("ru".to_string()),
        qualities,
        is_default: true,
    }]
}

/// Prefer the requested quality; fall back to the best available rendition.
fn select_quality<'a>(
    payload: &'a anilibria_hls::Payload,
    requested: Option<&str>,
) -> Result<&'a anilibria_hls::QualityRef, String> {
    let Some(first) = payload.qualities.first() else {
        return Err("player payload has no renditions".into());
    };
    let Some(requested) = requested else {
        return Ok(first);
    };
    Ok(payload
        .qualities
        .iter()
        .find(|quality| quality.label.eq_ignore_ascii_case(requested))
        .unwrap_or(first))
}

export!(Component);

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> anilibria_hls::Payload {
        anilibria_hls::Payload {
            episode_id: "e1".into(),
            qualities: vec![
                anilibria_hls::QualityRef {
                    label: "1080p".into(),
                    url: "https://cdn/1080.m3u8".into(),
                },
                anilibria_hls::QualityRef {
                    label: "720p".into(),
                    url: "https://cdn/720.m3u8".into(),
                },
            ],
            translations: vec!["Ditaro".into(), "Kroxxa".into(), "kaktus".into(), "Zozya".into()],
            referer: Some("https://anilibria.top/".into()),
        }
    }

    #[test]
    fn one_aniliberty_dub_ignores_legacy_member_names_and_keeps_actual_qualities() {
        let mut payload = payload();
        for translations in [payload.translations.clone(), vec![], vec!["AniLibria".into()]] {
            payload.translations = translations;
            let dubs = dubs(&payload);
            assert_eq!(dubs.len(), 1);
            assert_eq!(dubs[0].id, "0");
            assert_eq!(dubs[0].name, "Aniliberty");
            assert_eq!(dubs[0].language.as_deref(), Some("ru"));
            assert!(dubs[0].is_default);
            assert_eq!(dubs[0].qualities, vec!["1080p", "720p"]);
        }
    }

    #[test]
    fn legacy_payload_plays_the_single_dub_and_rejects_fake_dub_ids() {
        let payload = serde_json::to_string(&payload()).unwrap();
        let stream = Component::get_link(payload.clone(), "0".into(), Some("720p".into())).unwrap();
        let StreamData::Hls(stream) = stream else { panic!("expected HLS"); };
        assert_eq!(stream.url, "https://cdn/720.m3u8");
        assert!(Component::get_link(payload.clone(), "00".into(), None).is_ok());
        assert!(Component::get_link(payload, "1".into(), None).is_err());
    }

    #[test]
    fn quality_selection_falls_back_to_best() {
        let payload = payload();
        assert_eq!(
            select_quality(&payload, Some("720p")).unwrap().label,
            "720p"
        );
        assert_eq!(
            select_quality(&payload, Some("480p")).unwrap().label,
            "1080p"
        );
        assert_eq!(select_quality(&payload, None).unwrap().label, "1080p");
    }
}
