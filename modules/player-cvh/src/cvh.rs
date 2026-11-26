use super::animex::module::types::{Header, StreamDash, StreamData, StreamHls};
use anm_sdk::protocol::provider_embed::CvhContext;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    vk_id: Option<String>,
    voice_studio: Option<String>,
    season: Option<u32>,
    episode: Option<u32>,
}
#[derive(Deserialize)]
struct Playlist {
    items: Vec<Item>,
}

fn get(url: &str, referer: Option<&str>) -> Result<String, String> {
    let mut headers = vec![("accept".into(), "application/json".into())];
    if let Some(referer) = referer {
        headers.push(("referer".into(), referer.into()));
    }
    let response = anm_sdk::http_request(&anm_sdk::HttpRequest {
        url: url.into(),
        method: None,
        headers,
        body: None,
    })?;
    if !(200..300).contains(&response.status) {
        return Err(format!("CVH HTTP {}", response.status));
    }
    String::from_utf8(response.body).map_err(|e| e.to_string())
}

fn studio(value: &str) -> String {
    let value: String = value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    value.strip_suffix("tv").unwrap_or(&value).to_string()
}

fn choose<'a>(items: &'a [Item], context: &CvhContext) -> Result<&'a Item, String> {
    // Never silently substitute a different episode, season, or voice.
    let wanted_voice = studio(&context.voice);
    items
        .iter()
        .find(|item| {
            item.season == Some(context.season)
                && item.episode == Some(context.episode)
                && item
                    .voice_studio
                    .as_deref()
                    .is_some_and(|v| studio(v) == wanted_voice)
        })
        .ok_or_else(|| "CVH: requested season/episode/dub is unavailable".into())
}

pub fn resolve(context: &CvhContext, referer: Option<&str>) -> Result<StreamData, String> {
    if context.media_id.is_empty()
        || context.publisher_id.is_empty()
        || context.aggregator.is_empty()
        || context.season == 0
        || context.episode == 0
        || context.voice.trim().is_empty()
    {
        return Err("CVH: incomplete integration context".into());
    }
    let mut url =
        url::Url::parse("https://plapi.cdnvideohub.com/api/v1/player/sv/playlist").unwrap();
    url.query_pairs_mut().extend_pairs([
        ("pub", context.publisher_id.as_str()),
        ("aggr", context.aggregator.as_str()),
        ("id", context.media_id.as_str()),
    ]);
    let playlist: Playlist = serde_json::from_str(&get(url.as_str(), referer)?)
        .map_err(|e| format!("CVH playlist: {e}"))?;
    let id = choose(&playlist.items, context)?
        .vk_id
        .as_deref()
        .filter(|v| !v.is_empty())
        .ok_or("CVH: missing video id")?;
    let mut url = url::Url::parse("https://plapi.cdnvideohub.com/api/v1/player/sv/video/").unwrap();
    url.path_segments_mut()
        .map_err(|_| "CVH: invalid video URL")?
        .pop_if_empty()
        .push(id);
    let video: serde_json::Value = serde_json::from_str(&get(url.as_str(), referer)?)
        .map_err(|e| format!("CVH video: {e}"))?;
    stream_from_video(&video, referer)
}

fn stream_from_video(
    video: &serde_json::Value,
    referer: Option<&str>,
) -> Result<StreamData, String> {
    let headers: Vec<_> = referer
        .into_iter()
        .map(|value| Header {
            name: "Referer".into(),
            value: value.into(),
        })
        .collect();
    for (key, dash) in [("dashUrl", true), ("hlsUrl", false)] {
        let value = &video["sources"][key];
        let Some(value) = value
            .as_str()
            .or_else(|| value.get("src").and_then(|v| v.as_str()))
            .or_else(|| value.get("url").and_then(|v| v.as_str()))
            .or_else(|| value.get("value").and_then(|v| v.as_str()))
        else {
            continue;
        };
        if super::common::https_url(value).is_err() {
            continue;
        }
        return Ok(if dash {
            StreamData::Dash(StreamDash {
                url: value.into(),
                headers,
                subtitles: vec![],
            })
        } else {
            StreamData::Hls(StreamHls {
                url: value.into(),
                headers,
                qualities: vec![],
                subtitles: vec![],
            })
        });
    }
    Err("CVH: no playable HLS/DASH source".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_does_not_fall_back_to_another_episode_or_dub() {
        let items: Vec<Item> = serde_json::from_value(serde_json::json!([
            {"vkId":"wrong-season", "season":1,"episode":1,"voiceStudio":"AniLibria"},
            {"vkId":"correct", "season":2,"episode":1,"voiceStudio":"AniLibria"}
        ]))
        .unwrap();
        let mut context = CvhContext {
            media_id: "other-title".into(),
            publisher_id: "other-publisher".into(),
            aggregator: "other-site".into(),
            season: 2,
            episode: 1,
            voice: "AnilibriaTV".into(),
        };
        assert_eq!(
            choose(&items, &context).unwrap().vk_id.as_deref(),
            Some("correct")
        );
        context.episode = 2;
        assert!(choose(&items, &context).is_err());
        context.episode = 1;
        context.voice = "missing".into();
        assert!(choose(&items, &context).is_err());
    }
    #[test]
    fn prefers_dash_and_keeps_hls_as_fallback() {
        let video = serde_json::json!({"sources":{"hlsUrl":"https://cdn.example/a.m3u8","dashUrl":{"value":"https://cdn.example/a.mpd"}}});
        assert!(matches!(
            stream_from_video(&video, Some("https://other.example/")).unwrap(),
            StreamData::Dash(_)
        ));
        assert!(matches!(
            stream_from_video(
                &serde_json::json!({"sources":{"hlsUrl":"https://cdn.example/a.m3u8"}}),
                None
            )
            .unwrap(),
            StreamData::Hls(_)
        ));
        assert!(stream_from_video(&serde_json::json!({}), None).is_err());
    }
}
