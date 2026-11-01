//! AnimeGO embed player module (`org.animego.player.embed`).
//!
//! Implements the WIT `player-api` interface for the third-party protocol
//! `animego.embed.v1`: the source module emits one candidate per dub with an
//! opaque JSON payload, and this player resolves it to a stream.
//!
//! Payload shape (JSON):
//! `{ "player": "aniboom|cvh|kodik|...", "embed": url, "label": dub,
//!    "translationId": id?, "episode": n, "cvhId": id?, "referer": url }`
//!
//! Resolution flows (verified against anime-dl-core, 2026):
//! - aniboom: the embed page carries `<video ... data-parameters="...">`, an
//!   html-escaped JSON with `hls`/`dash`/`fallbackHls`/`fallbackDash` entries
//!   of the form `{"src": url, "type": mime}`. Playback needs
//!   `Referer: https://aniboom.one/` and `Origin: https://aniboom.one`.
//! - cvh (CdnVideoHub): hidden open API, `pub=747&aggr=mali` for AnimeGO.
//!   `/api/v1/player/sv/playlist?id={id}` lists episodes/dubs, `/video/{vkId}`
//!   returns `sources` with `hlsUrl`/`dashUrl`/MP4 urls (okcdn.ru).
//! - kodik: requires a per-site token; deliberately not supported yet.

wit_bindgen::generate!({
    path: "../../vendor/sdk/wit",
    world: "player-module",
});

use animex::module::types::{Dub, Header, PlayerCapabilities, Quality, StreamData, StreamHls};
use exports::animex::module::player_api::Guest;
use serde::Deserialize;

const PROTOCOL: &str = "animego.embed.v1";

const CVH_API_BASE: &str = "https://plapi.cdnvideohub.com/api/v1/player/sv";
const CVH_PUB: &str = "747";
const CVH_AGGR: &str = "mali";
const CVH_REFERER: &str = "https://animego.org/";
const ANIBOOM_REFERER: &str = "https://aniboom.one/";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

// ---------------------------------------------------------------------------
// Payload model.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct EmbedPayload {
    player: Option<String>,
    #[serde(alias = "provider")]
    provider: Option<String>,
    embed: Option<String>,
    #[serde(alias = "label")]
    label: Option<String>,
    #[serde(alias = "translationId")]
    translation_id: Option<String>,
    episode: Option<u32>,
    #[serde(alias = "cvhId")]
    cvh_id: Option<String>,
    referer: Option<String>,
    #[serde(alias = "animegoId")]
    animego_id: Option<String>,
}

fn parse_payload(payload: &str) -> Result<EmbedPayload, String> {
    let parsed: EmbedPayload = serde_json::from_str(payload)
        .map_err(|e| format!("animego.embed.v1 payload decode: {e}"))?;
    let has_embed = parsed
        .embed
        .as_deref()
        .is_some_and(|embed| !embed.trim().is_empty());
    if !has_embed {
        return Err("animego.embed.v1 payload has no embed url".into());
    }
    Ok(parsed)
}

/// Lowercased provider of the payload.
fn player_kind(payload: &EmbedPayload) -> String {
    payload
        .player
        .as_deref()
        .or(payload.provider.as_deref())
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

/// Static capability lists per provider.
fn qualities_for(kind: &str) -> Vec<String> {
    let static_list = match kind {
        "cvh" => ["1080p", "720p", "480p", "360p", "240p"].as_slice(),
        "aniboom" => ["1080p", "720p", "480p", "360p"].as_slice(),
        "kodik" => ["720p", "480p", "360p"].as_slice(),
        _ => [].as_slice(),
    };
    static_list.iter().map(|value| value.to_string()).collect()
}

// ---------------------------------------------------------------------------
// Host HTTP helper.
// ---------------------------------------------------------------------------

fn get_body(url: &str, headers: Vec<(&str, String)>) -> Result<String, String> {
    let mapped: Vec<(String, String)> = headers
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect();
    let response = anm_sdk::http_request(&anm_sdk::HttpRequest {
        url: url.to_string(),
        method: None,
        headers: mapped,
        body: None,
    })?;
    if !(200..300).contains(&response.status) {
        return Err(format!("http status {} for {url}", response.status));
    }
    Ok(String::from_utf8_lossy(&response.body).to_string())
}

// ---------------------------------------------------------------------------
// Aniboom embed page.
// ---------------------------------------------------------------------------

/// `data-parameters` is html-escaped JSON (`&quot;` for quotes). Find the
/// attribute value bounded by quote characters after `=`.
fn extract_attribute(html: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = html.find(&needle)? + needle.len();
    let quote = html[start..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &html[start + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

/// Json sources carried by the aniboom `<video>` tag.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct AniboomParams {
    hls: Option<String>,
    dash: Option<String>,
    #[serde(alias = "fallbackHls")]
    fallback_hls: Option<String>,
    #[serde(alias = "fallbackDash")]
    fallback_dash: Option<String>,
}

/// The `hls` values arrive as a JSON string `{"src": url, "type": mime}` or as
/// a plain URL.
fn src_of(value: Option<&str>) -> Option<String> {
    let value = value?;
    if value.starts_with("http") {
        return Some(value.to_string());
    }
    #[derive(Deserialize)]
    struct Src {
        src: Option<String>,
    }
    serde_json::from_str::<Src>(value)
        .ok()
        .and_then(|parsed| parsed.src)
        .filter(|src| src.starts_with("http"))
}

/// Simplified unescape for the aniboom parameter payload: the attribute value
/// carries `&quot;`-escaped quotes; `&amp;` appears inside URLs.
fn unescape_parameters(input: &str) -> String {
    input
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#038;", "&")
        .replace("&amp;", "&")
}

/// Pure parse of an aniboom embed page body (network-free, testable).
fn aniboom_parse(body: &str) -> Result<StreamData, String> {
    let parameters = extract_attribute(body, "data-parameters")
        .ok_or_else(|| "aniboom page has no data-parameters".to_string())?;
    let unescaped = unescape_parameters(&parameters);
    let params: AniboomParams = serde_json::from_str(&unescaped)
        .map_err(|e| format!("aniboom data-parameters decode: {e}: {unescaped}"))?;
    let url = src_of(params.hls.as_deref())
        .or_else(|| src_of(params.fallback_hls.as_deref()))
        .or_else(|| src_of(params.dash.as_deref()))
        .or_else(|| src_of(params.fallback_dash.as_deref()))
        .ok_or_else(|| "aniboom returned no hls/dash".to_string())?;
    Ok(StreamData::Hls(StreamHls {
        url,
        headers: vec![
            Header { name: "Referer".to_string(), value: ANIBOOM_REFERER.to_string() },
            Header { name: "Origin".to_string(), value: ANIBOOM_REFERER.trim_end_matches('/').to_string() },
        ],
        qualities: Vec::new(),
        subtitles: Vec::new(),
    }))
}

/// Resolve an aniboom embed to the master HLS url (fallback: dash).
fn aniboom_stream(embed: &str, referer: Option<&str>) -> Result<StreamData, String> {
    let headers = vec![
        ("user-agent", USER_AGENT.to_string()),
        ("referer", referer.unwrap_or("https://animego.org/").to_string()),
    ];
    let body = get_body(embed, headers)?;
    aniboom_parse(&body).map_err(|e| format!("aniboom page {embed}: {e}"))
}

// ---------------------------------------------------------------------------
// CVH (CdnVideoHub) API.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct CvhPlaylist {
    items: Vec<CvhItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct CvhItem {
    #[serde(alias = "cvhId")]
    cvh_id: Option<String>,
    #[serde(alias = "vkId")]
    vk_id: Option<String>,
    #[serde(alias = "voiceStudio")]
    voice_studio: Option<String>,
    #[serde(alias = "voiceType")]
    voice_type: Option<String>,
    season: Option<u32>,
    episode: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct CvhVideo {
    sources: Option<CvhSources>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct CvhSources {
    #[serde(alias = "hlsUrl")]
    hls_url: Option<serde_json::Value>,
    #[serde(alias = "dashUrl")]
    dash_url: Option<serde_json::Value>,
    url: Option<String>,
}

impl CvhSources {
    /// CVH returns either plain strings or `{"value": url}` objects; accept
    /// both but never admit non-URL values.
    fn url_of(value: &Option<serde_json::Value>) -> Option<String> {
        match value {
            Some(serde_json::Value::String(url)) => Some(url.clone()),
            Some(map @ serde_json::Value::Object(_)) => map
                .get("src")
                .and_then(|value| value.as_str().map(str::to_string))
                .or_else(|| {
                    map.get("url")
                        .and_then(|value| value.as_str().map(str::to_string))
                }),
            _ => None,
        }
        .filter(|url| url.starts_with("http"))
    }
}

/// Loose dub matching, per anime-dl-core `_match_studio`: case-insensitive,
/// punctuation ignored, exact first then substring containment.
fn studio_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

fn match_studio<'a>(label: &str, items: &'a [CvhItem]) -> Option<&'a CvhItem> {
    let wanted = studio_key(label);
    if wanted.is_empty() {
        return None;
    }
    items
        .iter()
        .find(|item| item.voice_studio.as_deref().is_some_and(|studio| studio_key(studio) == wanted))
        .or_else(|| {
            items.iter().find(|item| {
                item.voice_studio
                    .as_deref()
                    .is_some_and(|studio| studio_key(studio).contains(&wanted))
            })
        })
}

/// Resolve a CVH payload: playlist for the cvh id, pick the right season /
/// episode / studio entry, then fetch its video sources.
fn cvh_stream(payload: &EmbedPayload) -> Result<StreamData, String> {
    let cvh_id = payload
        .cvh_id
        .clone()
        .or_else(|| extract_cvh_id(payload.embed.as_deref().unwrap_or_default()))
        .ok_or_else(|| "cvh payload has no cvhId".to_string())?;
    let label = payload.label.as_deref().unwrap_or_default();
    let episode = payload.episode.unwrap_or(1);
    let season = 1;

    let playlist_url = format!("{CVH_API_BASE}/playlist?pub={CVH_PUB}&aggr={CVH_AGGR}&id={cvh_id}");
    let body = get_body(
        &playlist_url,
        vec![
            ("referer", CVH_REFERER.to_string()),
            ("accept", "application/json".to_string()),
            ("user-agent", USER_AGENT.to_string()),
        ],
    )?;
    let playlist: CvhPlaylist = serde_json::from_str(&body)
        .map_err(|e| format!("cvh playlist decode {cvh_id}: {e}"))?;
    if playlist.items.is_empty() {
        return Err(format!("cvh returned an empty playlist for id={cvh_id}"));
    }

    // Season selection: CVH keeps one season per media item in practice; when
    // several seasons appear, prefer the exact episode within the first
    // season that carries it.
    let mut pool: Vec<CvhItem> = playlist.items.clone();
    let seasons: Vec<u32> = {
        let mut seasons: Vec<u32> = pool.iter().filter_map(|item| item.season).collect();
        seasons.sort_unstable();
        seasons.dedup();
        seasons
    };
    if seasons.len() > 1 && seasons.contains(&season) {
        pool.retain(|item| item.season == Some(season));
    }
    let exact: Vec<CvhItem> = pool
        .iter()
        .filter(|item| item.episode.map(|value| value == episode).unwrap_or(false))
        .cloned()
        .collect();
    if !exact.is_empty() {
        pool = exact;
    }

    let chosen = if label.is_empty() {
        pool.first()
    } else {
        match_studio(label, &pool)
            .or_else(|| pool.first())
            .or_else(|| playlist.items.first())
    };
    let Some(chosen) = chosen else {
        return Err(format!("cvh: no matching dub for '{label}' episode {episode}"));
    };
    let Some(vk_id) = chosen.vk_id.clone().filter(|value| !value.is_empty()) else {
        return Err(format!("cvh: playlist item without vkId for '{label}' episode {episode}"));
    };

    let video_url = format!("{CVH_API_BASE}/video/{vk_id}");
    let body = get_body(
        &video_url,
        vec![
            ("referer", CVH_REFERER.to_string()),
            ("accept", "application/json".to_string()),
            ("user-agent", USER_AGENT.to_string()),
        ],
    )?;
    let video: CvhVideo = serde_json::from_str(&body)
        .map_err(|e| format!("cvh video decode {vk_id}: {e}"))?;
    let sources = video.sources.ok_or_else(|| format!("cvh video {vk_id} has no sources"))?;

    if let Some(hls) = CvhSources::url_of(&sources.hls_url) {
        return Ok(make_stream(hls, CvhSources::url_of(&sources.dash_url)));
    }
    if let Some(dash) = CvhSources::url_of(&sources.dash_url) {
        return Ok(StreamData::Dash(create_dash(dash)));
    }
    Err(format!("cvh returned no hls/dash for video {vk_id}"))
}

fn make_stream(hls: String, _dash: Option<String>) -> StreamData {
    StreamData::Hls(StreamHls {
        url: hls,
        headers: vec![Header { name: "Referer".to_string(), value: CVH_REFERER.to_string() }],
        qualities: Vec::new(),
        subtitles: Vec::new(),
    })
}

fn create_dash(url: String) -> animex::module::types::StreamDash {
    animex::module::types::StreamDash {
        url,
        headers: vec![Header { name: "Referer".to_string(), value: CVH_REFERER.to_string() }],
        subtitles: Vec::new(),
    }
}

/// `/cdn-iframe/{id}/...` — the numeric id is the CVH media id.
pub(crate) fn extract_cvh_id(embed: &str) -> Option<String> {
    const NEEDLE: &str = "cdn-iframe/";
    let start = embed.find(NEEDLE)? + NEEDLE.len();
    let rest = &embed[start..];
    let digits: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    (!digits.is_empty()).then_some(digits)
}

// ---------------------------------------------------------------------------
// Guest implementation.
// ---------------------------------------------------------------------------

struct Component;

impl Guest for Component {
    fn describe() -> Result<PlayerCapabilities, String> {
        Ok(PlayerCapabilities {
            qualities: ["1080p", "720p", "480p", "360p", "240p"]
                .iter()
                .map(|value| value.to_string())
                .collect(),
            protocols: vec![PROTOCOL.to_string()],
            languages: vec!["ru".to_string()],
        })
    }

    fn get_dubs(payload: String) -> Result<Vec<Dub>, String> {
        let payload = parse_payload(&payload)?;
        let kind = player_kind(&payload);
        let name = payload
            .label
            .clone()
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| "AnimeGO".to_string());
        Ok(vec![Dub {
            id: "0".to_string(),
            name,
            language: Some("ru".to_string()),
            qualities: qualities_for(&kind),
            is_default: true,
        }])
    }

    fn get_link(
        payload: String,
        _dub_id: String,
        _quality: Option<String>,
    ) -> Result<StreamData, String> {
        let payload = parse_payload(&payload)?;
        match player_kind(&payload).as_str() {
            "aniboom" => aniboom_stream(
                payload.embed.as_deref().unwrap_or_default(),
                payload.referer.as_deref(),
            ),
            "cvh" => cvh_stream(&payload),
            "kodik" => Err("kodik is not supported yet".into()),
            kind => Err(format!("unknown provider '{kind}' for embed {}", payload.embed.unwrap_or_default())),
        }
    }
}

export!(Component);

#[cfg(test)]
mod tests;
