use super::{
    animex::module::types::{Header, StreamDash, StreamData, StreamHls},
    common,
};

fn source(value: &serde_json::Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        if let Ok(inner) = serde_json::from_str::<serde_json::Value>(value) {
            return source(&inner);
        }
        return common::https_url(value).ok().map(|url| url.to_string());
    }
    value
        .get("src")
        .and_then(|value| value.as_str())
        .and_then(|value| common::https_url(value).ok())
        .map(|url| url.to_string())
}

fn parse(body: &str, embed: &str) -> Result<StreamData, String> {
    let raw = common::attribute(body, "data-parameters")
        .ok_or("AniBoom: no video parameters; embed may be unavailable")?;
    let data: serde_json::Value = serde_json::from_str(&common::unescape(&raw))
        .map_err(|_| "AniBoom: invalid video parameters")?;
    let origin = common::https_url(embed)?.origin().ascii_serialization();
    let headers = vec![
        Header {
            name: "Referer".into(),
            value: format!("{origin}/"),
        },
        Header {
            name: "Origin".into(),
            value: origin,
        },
    ];
    for (key, dash) in [
        ("hls", false),
        ("fallbackHls", false),
        ("dash", true),
        ("fallbackDash", true),
    ] {
        if let Some(url) = source(&data[key]) {
            return Ok(if dash {
                StreamData::Dash(StreamDash {
                    url,
                    headers,
                    subtitles: vec![],
                })
            } else {
                StreamData::Hls(StreamHls {
                    url,
                    headers,
                    qualities: vec![],
                    subtitles: vec![],
                })
            });
        }
    }
    Err("AniBoom: no direct video sources are available".into())
}

pub fn resolve(embed: &str, referer: Option<&str>) -> Result<StreamData, String> {
    parse(&common::request(embed, referer, None)?, embed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supports_nested_hls_parameters_and_a_real_dash_fallback() {
        let body = r#"<video data-parameters="{&quot;hls&quot;:&quot;{\&quot;src\&quot;:\&quot;https://cdn.example/master.m3u8?a=1&amp;b=2\&quot;}&quot;}"></video>"#;
        match parse(body, "https://aniboom.one/embed/1").unwrap() {
            StreamData::Hls(value) => {
                assert_eq!(value.url, "https://cdn.example/master.m3u8?a=1&b=2")
            }
            _ => panic!("expected HLS"),
        }
        assert!(matches!(parse(r#"<video data-parameters='{"hls":null,"dash":{"src":"https://cdn.example/video.mpd"}}'></video>"#, "https://aniboom.one/embed/1").unwrap(), StreamData::Dash(_)));
        assert!(parse("<h1>Unavailable</h1>", "https://aniboom.one/embed/1").is_err());
    }
}
