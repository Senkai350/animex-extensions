//! HTML helpers over `dom_query` (an html5ever-based DOM with CSS selectors).
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/Helper/Parser.php` — `idFromUrl`, `parseImageQuality`, date helpers,
//!   child-node stripping and text extraction patterns.
//! - `src/Helper/JString.php` — whitespace/NBSP/control-character cleansing.
//! - `src/Helper/ImagePlaceholder.php` — MAL placeholder image fragments.
//! - `src/Helper/Constants.php` — `Unknown` / `N/A` / `None found` sentinels.
//! - `src/Parser/Common/MalUrlParser.php`, `UrlParser.php` — link extraction.
//! - `src/Http/HttpClientWrapper.php` — non-2xx responses are errors, not pages.
//!
//! Symfony DomCrawler + XPath is replaced by CSS selectors. All selectors here
//! intentionally accept both the modern right-column layout and the older
//! compact manga/character layout that MAL still serves on some pages.

use dom_query::{Document, NodeRef, Selection};

use crate::model::{absolute_url, SITE};

/// Fragments of MAL's generic "no artwork" images. Copied from Jikan's
/// `ImagePlaceholder::PLACEHOLDER_FRAGMENTS` (measured against live MAL).
const IMAGE_PLACEHOLDERS: &[&str] = &[
    "/images/questionmark_23.gif",
    "/images/questionmark_50.gif",
    "/images/kaomoji_mal_white.png",
    "/images/qm_50.gif",
    "/images/company/placeholder.png",
    "/images/na.gif",
    "/images/na_series_",
    "/images/spacer.gif",
    "/img/sp/icon/apple-touch-icon-256.png",
];

/// Parse and lightly sanitize a MAL page. Scripts/styles and the explanatory
/// footnotes inside statistics rows are removed so label lookups return the
/// value only (Jikan does the same with `Parser::removeChildNodes`).
///
/// The byte-level pre-strip happens before html5ever sees the document: MAL
/// pages embed several hundred kilobytes of analytics/ad scripts, and not
/// tokenizing them keeps the module inside the host's per-call fuel budget.
pub fn document(body: &str) -> Document {
    let stripped = strip_heavy_nodes(body);
    let document = Document::from(stripped.as_str());
    let junk = document.select("script, style, noscript, sup, .statistics-info");
    if junk.exists() {
        junk.remove();
    }
    document
}

fn find_ci(haystack: &str, needle: &str, start: usize) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (start..=haystack.len() - needle.len())
        .find(|&index| haystack[index..index + needle.len()].eq_ignore_ascii_case(needle))
}

fn strip_heavy_nodes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let script = find_ci(input, "<script", cursor);
        let style = find_ci(input, "<style", cursor);
        let noscript = find_ci(input, "<noscript", cursor);
        let comment = find_ci(input, "<!--", cursor);
        let next = [script, style, noscript, comment]
            .into_iter()
            .flatten()
            .min();
        let Some(start) = next else {
            output.push_str(&input[cursor..]);
            break;
        };
        output.push_str(&input[cursor..start]);
        let (opening, closing) = if comment == Some(start) {
            ("<!--", "-->")
        } else if script == Some(start) {
            ("<script", "</script")
        } else if style == Some(start) {
            ("<style", "</style")
        } else {
            ("<noscript", "</noscript")
        };
        let after_open = start + opening.len();
        let end = if closing == "-->" {
            find_ci(input, closing, after_open)
                .map(|end| end + closing.len())
                .unwrap_or(input.len())
        } else {
            find_ci(input, closing, after_open)
                .and_then(|end| input[end..].find('>').map(|offset| end + offset + 1))
                .unwrap_or(input.len())
        };
        cursor = end;
    }
    output
}

/// Collapse whitespace, convert NBSP/zero-width characters to spaces, drop
/// control characters. Equivalent to `JString::cleanse` for our use cases.
pub fn clean_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut pending_space = false;
    for character in input.chars() {
        let character = match character {
            '\u{00a0}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}' => ' ',
            other => other,
        };
        if character.is_whitespace() {
            pending_space = !output.is_empty();
            continue;
        }
        if character.is_control() {
            continue;
        }
        if pending_space {
            output.push(' ');
            pending_space = false;
        }
        output.push(character);
    }
    output.trim().to_string()
}

pub fn node_text(node: &NodeRef<'_>) -> Option<String> {
    let text = clean_text(&node.text());
    (!text.is_empty()).then_some(text)
}

/// First non-empty text from a CSS selector.
pub fn select_text(document: &Document, selector: &str) -> Option<String> {
    document
        .select(selector)
        .nodes()
        .iter()
        .find_map(|node| node_text(node))
}

/// Attribute of the first matching element, trimmed.
pub fn select_attr(document: &Document, selector: &str, attribute: &str) -> Option<String> {
    document
        .select(selector)
        .nodes()
        .iter()
        .find_map(|node| node.attr(attribute).map(|value| value.to_string()))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Find the `.spaceit_pad` row belonging to a sidebar label (`Genres:`,
/// `Aired:`, `Episodes:` ...). MAL may use singular or plural spellings.
pub fn label_row<'a>(document: &'a Document, labels: &[&str]) -> Option<Selection<'a>> {
    document
        .select("span.dark_text")
        .nodes()
        .iter()
        .find_map(|label| {
            let text = clean_text(&label.text());
            let name = text.trim_end_matches(':').trim();
            if labels
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
            {
                label.parent().map(Selection::from)
            } else {
                None
            }
        })
}

/// Whole text of a label row with the label itself removed.
pub fn label_text(document: &Document, labels: &[&str]) -> Option<String> {
    let row = label_row(document, labels)?;
    let label = clean_text(&row.select_single("span.dark_text").text());
    let all = clean_text(&row.text());
    let rest = all.strip_prefix(label.trim()).unwrap_or(&all);
    let rest = clean_text(rest.trim_start_matches(':'));
    (!rest.is_empty()).then_some(rest)
}

/// Links inside a label row, in document order.
pub fn label_links(document: &Document, labels: &[&str]) -> Vec<(String, Option<String>)> {
    let Some(row) = label_row(document, labels) else {
        return vec![];
    };
    row.select("a")
        .nodes()
        .iter()
        .filter_map(|link| {
            let name = node_text(link)?;
            let href = link
                .attr("href")
                .and_then(|value| absolute_url(&value.to_string()));
            Some((name, href))
        })
        .collect()
}

/// A number from a label row (`Members:`, `Favorites:`, `Ranked:` ...).
pub fn label_count(document: &Document, labels: &[&str]) -> Option<u32> {
    let text = label_text(document, labels)?;
    crate::model::count_from_text(&text)
}

/// Image URL from an `<img>` element. Prefers lazy-load attributes, rejects
/// MAL placeholders, and upgrades MAL thumbnail paths to the original asset.
pub fn image_url(node: &NodeRef<'_>) -> Option<String> {
    let candidates = ["data-src", "data-srcset", "src"];
    for attribute in candidates {
        let Some(raw) = node.attr(attribute) else {
            continue;
        };
        let raw = raw.to_string();
        let value = if attribute == "data-srcset" {
            raw.split(',')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .next()
        } else {
            Some(raw.as_str())
        };
        if let Some(url) = value.and_then(normalize_image) {
            return Some(url);
        }
    }
    None
}

/// `Parser::parseImageQuality`: drop MAL's `/r/WxH/` resize prefix and the
/// `v`/`t`/`l` thumbnail suffix, then reject placeholder artwork.
pub fn normalize_image(raw: &str) -> Option<String> {
    let mut value = raw.trim().trim_matches('"').to_string();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("//") {
        value = format!("https:{value}");
    }
    let lower = value.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    if IMAGE_PLACEHOLDERS
        .iter()
        .any(|fragment| lower.contains(fragment))
    {
        return None;
    }
    if lower.starts_with("http://") {
        value = format!("https://{}", &value[7..]);
    }

    if let Some(position) = value.find("/r/") {
        let rest = &value[position + 3..];
        if let Some(slash) = rest.find('/') {
            let dimensions = &rest[..slash];
            let valid = dimensions.split_once('x').is_some_and(|(width, height)| {
                !width.is_empty()
                    && !height.is_empty()
                    && width.bytes().all(|byte| byte.is_ascii_digit())
                    && height.bytes().all(|byte| byte.is_ascii_digit())
            });
            if valid {
                value.replace_range(position..position + 3 + slash, "");
            }
        }
    }

    let (path, query) = match value.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (value.as_str(), None),
    };
    let path = ["v.jpg", "t.jpg", "l.jpg"]
        .iter()
        .find_map(|suffix| path.strip_suffix(suffix).map(|base| format!("{base}.jpg")))
        .unwrap_or_else(|| path.to_string());
    Some(match query {
        Some(query) => format!("{path}?{query}"),
        None => path,
    })
}

/// Keep the full-size picture link but still reject placeholders/invalid URLs.
pub fn normalize_gallery_image(raw: &str) -> Option<String> {
    let value = absolute_url(raw)?;
    let lower = value.to_ascii_lowercase();
    if IMAGE_PLACEHOLDERS
        .iter()
        .any(|fragment| lower.contains(fragment))
    {
        return None;
    }
    Some(value)
}

/// MAL pages always identify the site in the canonical/OG metadata. A login
/// wall or an empty response does not; callers treat that as a parser error.
pub fn looks_like_mal_page(body: &str) -> bool {
    if body.trim().is_empty() {
        return false;
    }
    let has_html = body.contains("<html") || body.contains("<!DOCTYPE html");
    let has_mal = body.contains("myanimelist.net") || body.contains("MyAnimeList");
    has_html && has_mal
}

/// Cloudflare/Incapsula interstitials must never be parsed as an empty page.
/// This is a detection-only path: we do not try to solve the challenge.
pub fn is_challenge_page(body: &str) -> bool {
    [
        "Just a moment...",
        "Attention Required! | Cloudflare",
        "Enable JavaScript and cookies to continue",
        "Checking your browser before accessing",
        "cf-chl-",
        "_cf_chl_opt",
        "Incapsula incident ID",
    ]
    .iter()
    .any(|marker| body.contains(marker))
}

/// Short, tag-free body excerpt for error messages (never the whole page).
pub fn excerpt(body: &str, limit: usize) -> String {
    let mut text = String::with_capacity(limit * 2);
    let mut in_tag = false;
    for character in body.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(character),
            _ => {}
        }
        if text.chars().count() >= limit * 3 {
            break;
        }
    }
    let text = clean_text(&text);
    text.chars().take(limit).collect()
}

/// Keep only the sections a parser actually reads. This is a cheap byte-level
/// slice (no HTML parsing) and matters for the host fuel budget: the search
/// page embeds a huge filter form, and character pages carry reviews/news after
/// the cast/staff sections.
pub fn trim_for<'a>(path: &str, body: &'a str) -> &'a str {
    if path.starts_with("/anime.php") {
        return slice_from_to(body, "<div class=\"js-categories-seasonal", "</table>")
            .unwrap_or(body);
    }
    if path.ends_with("/_/characters") {
        let start = find_ci(body, "<div class=\"anime-character-container", 0)
            .or_else(|| find_ci(body, "<div id=\"manga-character-container", 0))
            .or_else(|| find_ci(body, "<div class=\"manga-character-container", 0));
        if let Some(start) = start {
            let tail = &body[start..];
            let end = [
                "<h2>Reviews",
                "<h2 >Reviews",
                "id=\"recent_news\"",
                "id=\"recent_forum_discussion\"",
                "</body>",
            ]
            .iter()
            .filter_map(|marker| find_ci(tail, marker, 0))
            .min()
            .map(|offset| start + offset)
            .unwrap_or(body.len());
            return &body[start..end];
        }
        return body;
    }
    if !path.contains("/_/") {
        // Detail page: drop reviews/news/forum/featured content after the
        // recommendations carousel, which is the last fallback section.
        if let Some(end) = find_ci(body, "id=\"recent_news\"", 0) {
            return &body[..end];
        }
    }
    body
}

fn slice_from_to<'a>(body: &'a str, start_marker: &str, end_marker: &str) -> Option<&'a str> {
    let start = find_ci(body, start_marker, 0)?;
    let after = start + start_marker.len();
    let end = find_ci(body, end_marker, after)? + end_marker.len();
    Some(&body[start..end])
}

/// Absolute MAL url for a path, used by the HTTP layer.
pub fn absolute_path(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{SITE}{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_read_without_hidden_statistics_footnotes() {
        let doc = document(
            r#"<html><body>
            <div class="spaceit_pad"><span class="dark_text">Status:</span> Finished Airing</div>
            <div class="spaceit_pad"><span class="dark_text">Ranked:</span> #727<sup>2</sup>
              <div class="statistics-info info2">based on the <a href="/topanime.php">top anime</a> page</div>
            </div>
            <div class="spaceit_pad"><span class="dark_text">Members:</span> 3,173,820</div>
            </body></html>"#,
        );
        assert_eq!(
            label_text(&doc, &["Status"]).as_deref(),
            Some("Finished Airing")
        );
        assert_eq!(label_count(&doc, &["Ranked"]), Some(727));
        assert_eq!(label_count(&doc, &["Members"]), Some(3_173_820));
    }

    #[test]
    fn image_quality_and_placeholders_follow_jikan() {
        assert_eq!(
            normalize_image("https://cdn.myanimelist.net/r/50x70/images/anime/1/20l.jpg?s=abc")
                .as_deref(),
            Some("https://cdn.myanimelist.net/images/anime/1/20.jpg?s=abc")
        );
        assert_eq!(
            normalize_image("https://cdn.myanimelist.net/r/42x62/images/questionmark_23.gif")
                .as_deref(),
            None
        );
        assert_eq!(
            normalize_image("https://cdn.myanimelist.net/images/anime/1141/142503.jpg").as_deref(),
            Some("https://cdn.myanimelist.net/images/anime/1141/142503.jpg")
        );
        assert_eq!(
            normalize_image("https://cdn.myanimelist.net/images/spacer.gif"),
            None
        );
    }

    #[test]
    fn links_ignore_javascript_and_find_relation_ids() {
        assert_eq!(
            absolute_url("/anime/20/Naruto").as_deref(),
            Some("https://myanimelist.net/anime/20/Naruto")
        );
        assert_eq!(absolute_url("javascript:void(0)"), None);
        assert_eq!(
            crate::model::id_from_href("https://myanimelist.net/manga/2/Berserk", "manga"),
            Some(2)
        );
    }

    #[test]
    fn challenge_pages_are_not_mistaken_for_mal_pages() {
        assert!(is_challenge_page("Just a moment..."));
        assert!(!is_challenge_page(
            r#"<html><body>Naruto - MyAnimeList.net</body></html>"#
        ));
        assert!(looks_like_mal_page(
            r#"<html><body>Naruto - MyAnimeList.net</body></html>"#
        ));
    }

    #[test]
    fn heavy_scripts_and_comments_are_stripped_before_html5ever() {
        let body = r#"<html><head><script>bad()</script><style>.x{}</style></head>
        <body><!-- advertisement --><p>visible</p><noscript>hidden</noscript></body></html>"#;
        let stripped = strip_heavy_nodes(body);
        assert!(!stripped.contains("bad()"));
        assert!(!stripped.contains(".x{}"));
        assert!(!stripped.contains("advertisement"));
        assert!(!stripped.contains("hidden"));
        assert!(stripped.contains("visible"));
    }

    #[test]
    fn page_trimming_keeps_only_parser_sections() {
        let search = r#"<html><body><form><option>huge select</option></form>
        <div class="js-categories-seasonal list"><table><tr><td>result</td></tr></table></div>
        <footer>noise</footer></body></html>"#;
        let trimmed = trim_for("/anime.php?q=x", search);
        assert!(trimmed.contains("result"));
        assert!(!trimmed.contains("huge select"));
        assert!(!trimmed.contains("noise"));

        let characters = r#"<html><body><div class="anime-character-container"><table>cast</table></div>
        <a name="staff"></a><table>staff</table><h2>Reviews</h2><p>noise</p></body></html>"#;
        let trimmed = trim_for("/anime/20/_/characters", characters);
        assert!(trimmed.contains("cast"));
        assert!(trimmed.contains("staff"));
        assert!(!trimmed.contains("noise"));
    }
}
