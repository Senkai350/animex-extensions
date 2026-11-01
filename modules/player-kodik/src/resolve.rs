use super::{
    animex::module::types::{Header, Quality, StreamData, StreamHls},
    common,
};
use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
    Engine as _,
};

/// Read string literals only. Provider JavaScript is never evaluated.
fn assigned(body: &str, key: &str) -> Option<String> {
    let start = body.find(key)? + key.len();
    let rest = body[start..].trim_start().strip_prefix('=')?.trim_start();
    let quote = rest.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let mut out = String::new();
    let mut escape = false;
    for character in rest[1..].chars() {
        if escape {
            out.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escape = false;
        } else if character == '\\' {
            escape = true;
        } else if character == quote {
            return Some(out);
        } else {
            out.push(character);
        }
    }
    None
}

fn post_form(body: &str) -> Result<String, String> {
    let params = assigned(body, "urlParams").ok_or("Kodik: embed has no request parameters")?;
    let params: serde_json::Value =
        serde_json::from_str(&params).map_err(|_| "Kodik: invalid request parameters")?;
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for key in ["type", "hash", "id"] {
        let value = ["vInfo", "videoInfo"]
            .iter()
            .find_map(|name| assigned(body, &format!("{name}.{key}")))
            .ok_or("Kodik: embed has no video identity")?;
        form.append_pair(key, &value);
    }
    for key in ["d", "d_sign", "pd", "pd_sign", "ref", "ref_sign"] {
        let value = params[key]
            .as_str()
            .ok_or("Kodik: missing request signature")?;
        let decoded;
        let value = if key == "ref" {
            decoded =
                url::form_urlencoded::parse(format!("v={}", value.replace('+', "%2B")).as_bytes())
                    .next()
                    .map(|(_, value)| value.into_owned())
                    .unwrap_or_default();
            decoded.as_str()
        } else {
            value
        };
        form.append_pair(key, value);
    }
    form.append_pair("bad_user", "true")
        .append_pair("cdn_is_working", "true");
    Ok(form.finish())
}

fn endpoint(script: &str, embed: &url::Url) -> Result<String, String> {
    let pattern = regex::Regex::new(r#"url\s*:\s*atob\(\s*["']([^"']+)["']"#).unwrap();
    let encoded = pattern
        .captures(script)
        .and_then(|capture| capture.get(1))
        .ok_or("Kodik: player endpoint not found")?
        .as_str();
    let path = String::from_utf8(
        STANDARD
            .decode(encoded)
            .or_else(|_| STANDARD_NO_PAD.decode(encoded))
            .map_err(|_| "Kodik: invalid endpoint encoding")?,
    )
    .map_err(|_| "Kodik: invalid endpoint text")?;
    let target = embed
        .join(&path)
        .map_err(|_| "Kodik: invalid endpoint URL")?;
    if !path.starts_with('/') || path.starts_with("//") || target.origin() != embed.origin() {
        return Err("Kodik: unexpected endpoint origin".into());
    }
    Ok(target.to_string())
}

fn direct_url(value: &str) -> Option<String> {
    let normalized = if value.starts_with("//") {
        format!("https:{value}")
    } else if let Some(rest) = value.strip_prefix("http://") {
        format!("https://{rest}")
    } else {
        value.into()
    };
    common::https_url(&normalized)
        .ok()
        .map(|url| url.to_string())
}

fn decode(value: &str) -> Result<String, String> {
    if let Some(url) = direct_url(value) {
        return Ok(url);
    }
    for shift in 0..26 {
        let rotated: String = value
            .bytes()
            .map(|byte| match byte {
                b'a'..=b'z' => (b'a' + (byte - b'a' + shift) % 26) as char,
                b'A'..=b'Z' => (b'A' + (byte - b'A' + shift) % 26) as char,
                byte => byte as char,
            })
            .collect();
        if let Ok(bytes) = STANDARD
            .decode(&rotated)
            .or_else(|_| STANDARD_NO_PAD.decode(&rotated))
        {
            if let Ok(value) = std::str::from_utf8(&bytes) {
                if let Some(url) = direct_url(value) {
                    return Ok(url);
                }
            }
        }
    }
    Err("Kodik: direct URL encoding is unsupported".into())
}

fn parse_links(body: &str, embed: &str, quality: Option<&str>) -> Result<StreamData, String> {
    let answer: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "Kodik: invalid video response")?;
    let entries = answer["links"]
        .as_object()
        .ok_or("Kodik: no direct links returned")?;
    let mut qualities = Vec::new();
    for (label, variants) in entries {
        let Ok(height) = label.parse::<u32>() else {
            continue;
        };
        if !(144..=4320).contains(&height) {
            continue;
        }
        let Some(source) = variants
            .as_array()
            .and_then(|items| items.iter().find_map(|item| item["src"].as_str()))
        else {
            continue;
        };
        qualities.push((
            height,
            Quality {
                label: format!("{height}p"),
                url: decode(source)?,
            },
        ));
    }
    qualities.sort_by(|a, b| b.0.cmp(&a.0));
    let qualities: Vec<_> = qualities.into_iter().map(|(_, quality)| quality).collect();
    let selected = match quality {
        Some(label) => qualities.iter().find(|quality| quality.label == label),
        None => qualities.first(),
    }
    .ok_or("Kodik: requested quality is unavailable")?;
    let origin = common::https_url(embed)?.origin().ascii_serialization();
    Ok(StreamData::Hls(StreamHls {
        url: selected.url.clone(),
        qualities,
        subtitles: vec![],
        headers: vec![Header {
            name: "Referer".into(),
            value: format!("{origin}/"),
        }],
    }))
}

pub fn resolve(
    embed: &str,
    referer: Option<&str>,
    quality: Option<&str>,
) -> Result<StreamData, String> {
    let base = common::https_url(embed)?;
    let page = common::request(embed, referer, None)?;
    let form = post_form(&page)?;
    let scripts =
        regex::Regex::new(r#"<script[^>]+src=["']([^"']*assets/js/app\.player[^"']+\.js)["']"#)
            .unwrap();
    let script = scripts
        .captures(&page)
        .and_then(|capture| capture.get(1))
        .ok_or("Kodik: player script not found")?
        .as_str();
    let script_url = base.join(script).map_err(|_| "Kodik: invalid script URL")?;
    if script_url.origin() != base.origin() {
        return Err("Kodik: unexpected script origin".into());
    }
    let script = common::request(script_url.as_str(), Some(embed), None)?;
    let target = endpoint(&script, &base)?;
    let answer = common::request(&target, Some(embed), Some(form.into_bytes()))?;
    parse_links(&answer, embed, quality)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_rotated_links_and_selects_the_exact_quality() {
        let raw = STANDARD.encode("//cdn.example/720.mp4:hls:manifest.m3u8");
        let encoded: String = raw
            .bytes()
            .map(|byte| match byte {
                b'a'..=b'z' => (b'a' + (byte - b'a' + 13) % 26) as char,
                b'A'..=b'Z' => (b'A' + (byte - b'A' + 13) % 26) as char,
                byte => byte as char,
            })
            .collect();
        assert_eq!(
            decode(&encoded).unwrap(),
            "https://cdn.example/720.mp4:hls:manifest.m3u8"
        );
        let answer = serde_json::json!({"links":{"720":[{"src":encoded}],"360":[{"src":"//cdn.example/360.m3u8"}]}}).to_string();
        match parse_links(
            &answer,
            "https://kodikplayer.com/seria/1/a/720p",
            Some("360p"),
        )
        .unwrap()
        {
            StreamData::Hls(value) => {
                assert_eq!(value.url, "https://cdn.example/360.m3u8");
                assert_eq!(value.qualities.len(), 2);
            }
            _ => panic!("expected HLS"),
        }
        assert!(parse_links(
            &answer,
            "https://kodikplayer.com/seria/1/a/720p",
            Some("1080p")
        )
        .is_err());
    }
    #[test]
    fn preserves_blank_ref_and_rejects_cross_origin_posts() {
        let body = r#"var urlParams='{"d":"source.example","d_sign":"x","pd":"kodikplayer.com","pd_sign":"y","ref":"","ref_sign":"z"}';var vInfo={};vInfo.type='seria';vInfo.hash='hash';vInfo.id='1';"#;
        let form = post_form(body).unwrap();
        assert!(form.contains("ref=&"));
        let base = url::Url::parse("https://kodikplayer.com/seria/1/a/720p").unwrap();
        assert_eq!(
            endpoint("url:atob('L2Z0b3I=')", &base).unwrap(),
            "https://kodikplayer.com/ftor"
        );
        assert!(endpoint(
            &format!("url:atob('{}')", STANDARD.encode("//evil.example/collect")),
            &base
        )
        .is_err());
        assert!(post_form("<div>Unavailable</div>").is_err());
    }
}
