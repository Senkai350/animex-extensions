//! Title picture gallery.
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/Parser/Common/PicturesPageParser.php` — `a.js-picture-gallery`.
//! - `src/Parser/Common/PictureParser.php` — original `href` plus lazy image.
//! - `src/Parser/Common/DefaultPicturesPageParser.php` — character/person
//!   variant of the same gallery.
//! - `src/Request/Anime/AnimePicturesRequest.php` — `/anime/<id>/pics`.
//!
//! MAL has no public episode-screenshot gallery; the Pictures tab is the only
//! image collection it exposes. The AniMex ABI calls this method
//! `get-screenshots`, so the gallery is returned there with the large `href` as
//! `original` and the lazy-loaded thumbnail as `preview`. Jikan documents the
//! same distinction (`pictures` are artwork, episode streams are separate).

use crate::animex::module::types::Screenshot;
use dom_query::Document;
use std::collections::HashSet;

use crate::html::{image_url, normalize_gallery_image};

pub fn pictures(document: &Document) -> Vec<Screenshot> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for link in document
        .select("a.js-picture-gallery")
        .nodes()
        .iter()
        .cloned()
    {
        let Some(original) = link
            .attr("href")
            .and_then(|href| normalize_gallery_image(&href.to_string()))
        else {
            continue;
        };
        if !seen.insert(original.clone()) {
            continue;
        }
        let preview = link
            .first_element_child()
            .and_then(|image| image.is_element().then_some(image))
            .and_then(|image| image_url(&image))
            .or_else(|| {
                // Some templates put the image directly under the anchor.
                link.element_children()
                    .into_iter()
                    .find_map(|child| image_url(&child))
            });
        result.push(Screenshot { original, preview });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::document;

    #[test]
    fn gallery_keeps_large_original_and_small_preview() {
        let pictures = pictures(&document(include_str!(
            "../../tests/fixtures/pictures.html"
        )));
        assert_eq!(pictures.len(), 2);
        assert_eq!(
            pictures[0].original,
            "https://cdn.myanimelist.net/images/anime/1/20l.jpg"
        );
        assert_eq!(
            pictures[0].preview.as_deref(),
            Some("https://cdn.myanimelist.net/images/anime/1/20.jpg")
        );
    }

    #[test]
    fn placeholder_pictures_are_not_returned() {
        let document = document(
            r#"<html><body>
            <a class="js-picture-gallery" href="https://cdn.myanimelist.net/images/questionmark_23.gif"><img src="https://cdn.myanimelist.net/images/questionmark_23.gif"></a>
            </body></html>"#,
        );
        assert!(pictures(&document).is_empty());
    }
}
