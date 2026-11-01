//! Offline tests for the animego.embed.v1 player.

use super::*;
// Guest trait so `Component.{describe,get_dubs,get_link}` resolve in tests.
use super::exports::animex::module::player_api::Guest as _;

// --- payload ------------------------------------------------------------------

const ANIBOOM_HTML: &str = r#"
<div class="player">
    <video id="video" data-parameters="{&quot;hls&quot;:&quot;{\&quot;src\&quot;:\&quot;https://xxx.aniboom.one/hls/abc/master.m3u8\&quot;,\&quot;type\&quot;:\&quot;application/vnd.apple.mpegurl\&quot;}&quot;,&quot;dash&quot;:null,&quot;fallbackHls&quot;:null,&quot;fallbackDash&quot;:null}" src=""></video>
</div>
"#;

fn anziboom_payload() -> String {
    serde_json::json!({
        "player": "aniboom",
        "embed": "https://aniboom.one/embed/AAAA1111?episode=1&translation=609",
        "label": "AniDUB",
        "translationId": "609",
        "episode": 1,
        "referer": "https://animego.me/",
    })
    .to_string()
}

fn cvh_payload() -> String {
    serde_json::json!({
        "player": "cvh",
        "embed": "https://animego.me/cdn-iframe/51019/1/1?dubbing=AnilibriaTV",
        "label": "AnilibriaTV",
        "translationId": "801",
        "episode": 1,
        "cvhId": "51019",
        "referer": "https://animego.me/",
    })
    .to_string()
}

#[test]
fn payload_decodes_with_aliases() {
    let payload = parse_payload(&anziboom_payload()).unwrap();
    assert_eq!(player_kind(&payload), "aniboom");
    assert_eq!(payload.label.as_deref(), Some("AniDUB"));
    // provider alias works when `player` is absent
    let alias: String = serde_json::json!({ "provider": "CVH", "embed": "https://x" }).to_string();
    assert_eq!(player_kind(&parse_payload(&alias).unwrap()), "cvh");
}

#[test]
fn payload_requires_embed() {
    assert!(parse_payload(r#"{"player": "aniboom"}"#).is_err());
}

// --- dubs -----------------------------------------------------------------------

#[test]
fn get_dubs_returns_one_dub_with_qualities() {
    let dubs = Component::get_dubs(anziboom_payload()).unwrap();
    assert_eq!(dubs.len(), 1);
    assert_eq!(dubs[0].name, "AniDUB");
    assert!(dubs[0].is_default);
    assert_eq!(dubs[0].id, "0");
    assert!(dubs[0].qualities.contains(&"1080p".to_string()));

    let dubs = Component::get_dubs(cvh_payload()).unwrap();
    assert_eq!(dubs[0].name, "AnilibriaTV");
    assert!(dubs[0].qualities.contains(&"240p".to_string()));
}

#[test]
fn describe_capabilities_stub() {
    let capabilities = Component::describe().unwrap();
    assert_eq!(capabilities.protocols, vec![PROTOCOL.to_string()]);
    assert!(capabilities.languages.contains(&"ru".to_string()));
}

// --- aniboom page parsing ---------------------------------------------------------

#[test]
fn extract_attribute_reads_quoted_values() {
    let html = r#"<video data-parameters='{"a":1}' src=""></video>"#;
    assert_eq!(extract_attribute(html, "data-parameters").as_deref(), Some(r#"{"a":1}"#));
}

#[test]
fn unescape_parameters_restores_json() {
    let escaped = r#"{&quot;hls&quot;:null}"#;
    assert_eq!(unescape_parameters(escaped), r#"{"hls":null}"#);
}

#[test]
fn unescape_parameters_last_amp_first() {
    // &amp; must be decoded last so &quot; (which starts with &) survives.
    let escaped = "{&quot;a&quot;:&quot;b&amp;c&quot;}";
    assert_eq!(unescape_parameters(escaped), r#"{"a":"b&c"}"#);
}

#[test]
fn src_of_accepts_plain_url_and_json_string() {
    assert_eq!(
        src_of(Some("https://cdn/x.m3u8")).as_deref(),
        Some("https://cdn/x.m3u8")
    );
    let inner = r#"{"src":"https://cdn/y.m3u8","type":"application/vnd.apple.mpegurl"}"#;
    assert_eq!(src_of(Some(inner)).as_deref(), Some("https://cdn/y.m3u8"));
    assert_eq!(src_of(Some("{\"src\":null}")), None);
    assert_eq!(src_of(None), None);
}

#[test]
fn aniboom_page_without_parameters_is_an_error() {
    let error = aniboom_parse("<div>no player here</div>").unwrap_err();
    assert!(error.contains("data-parameters"), "{error}");
}

#[test]
fn aniboom_parse_returns_master_hls() {
    let stream = aniboom_parse(ANIBOOM_HTML).expect("parses");
    match stream {
        StreamData::Hls(hls) => {
            assert_eq!(hls.url, "https://xxx.aniboom.one/hls/abc/master.m3u8");
            assert!(hls.headers.iter().any(|header| header.name == "Referer"));
        }
        _ => panic!("expected hls"),
    }
}

// --- cvh ----------------------------------------------------------------------

#[test]
fn extract_cvh_id_from_iframe_url() {
    assert_eq!(
        extract_cvh_id("https://animego.me/cdn-iframe/51019/1/1?dubbing=AnilibriaTV"),
        Some("51019".to_string())
    );
    assert_eq!(extract_cvh_id("https://aniboom.one/embed/x"), None);
}

#[test]
fn match_studio_is_loose() {
    let items = vec![
        CvhItem { voice_studio: Some("AnilibriaTV".into()), ..Default::default() },
        CvhItem { voice_studio: Some("СВ Дубль".into()), ..Default::default() },
    ];
    assert_eq!(
        match_studio("AniLibria", &items).and_then(|item| item.voice_studio.clone()),
        Some("AnilibriaTV".to_string())
    );
    assert_eq!(
        match_studio("св-дубль", &items).and_then(|item| item.voice_studio.clone()),
        Some("СВ Дубль".to_string())
    );
    assert!(match_studio("nope", &items).is_none());
}

#[test]
fn cvh_sources_accept_plain_and_object_urls() {
    let sources = CvhSources {
        hls_url: serde_json::json!("https://cdn/hls.m3u8").into(),
        dash_url: serde_json::json!({"src": "https://cdn/dash.mpd"}).into(),
        url: None,
    };
    assert_eq!(
        CvhSources::url_of(&sources.hls_url).as_deref(),
        Some("https://cdn/hls.m3u8")
    );
    assert_eq!(
        CvhSources::url_of(&sources.dash_url).as_deref(),
        Some("https://cdn/dash.mpd")
    );
    let junk = CvhSources {
        hls_url: serde_json::json!(42).into(),
        dash_url: None,
        url: None,
    };
    assert_eq!(CvhSources::url_of(&junk.hls_url), None);
}

// --- unsupported paths -----------------------------------------------------------

#[test]
fn kodik_is_explicitly_unsupported() {
    let payload = serde_json::json!({
        "player": "kodik",
        "embed": "https://kodikplayer.com/serial/1304528/abc123/720p",
        "label": "Студия",
        "episode": 1,
    })
    .to_string();
    let error = Component::get_link(payload, "0".to_string(), None).unwrap_err();
    assert!(error.contains("kodik"), "{error}");
}

#[test]
fn unknown_provider_is_an_error() {
    let payload = serde_json::json!({
        "player": "sibnet",
        "embed": "https://video.sibnet.ru/shell.php?videoid=1",
        "label": "Sibnet",
        "episode": 1,
    })
    .to_string();
    assert!(Component::get_link(payload, "0".to_string(), None).is_err());
}

#[test]
fn broken_payload_errors_cleanly() {
    assert!(Component::get_link("not-json".to_string(), "0".into(), None).is_err());
    assert!(Component::get_link(r#"{"embed":""}"#.to_string(), "0".into(), None).is_err());
}
