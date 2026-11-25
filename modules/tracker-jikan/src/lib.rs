//! First-party MyAnimeList tracker: direct HTML scraping of myanimelist.net.
//!
//! This module replaces the former jikan-edge REST client with a parser that
//! mirrors the original jikan-me/jikan library. The repository is split by
//! responsibility and every file names the upstream parsers it references:
//!
//! - `http.rs`   ← `src/Http/HttpClientWrapper.php`,
//!                 `src/Exception/BadResponseException.php`, and jikan-rest's
//!                 `DefaultCachedScraperService` / `UpdateCacheJob` policy.
//! - `html.rs`   ← `src/Helper/Parser.php`, `JString.php`,
//!                 `ImagePlaceholder.php`, `Constants.php`.
//! - `model.rs`  ← `src/Model/Common/MalUrl.php`, `Model/Anime/Anime.php`.
//! - `parse/*`   ← the parser listed in each module header.
//! - this file   ← `src/MyAnimeList/MalClient.php` (public operations and
//!                 error semantics) plus AniMex's WIT tracker contract.
//!
//! Anti-abuse policy (why this does not hammer MAL):
//! - long TTLs copied from Jikan REST (24 h titles/sections, 5 days search);
//! - 2-second shared spacing persisted across plugin instances;
//! - `If-None-Match` / `If-Modified-Since` revalidation instead of full GETs;
//! - 7-day negative cache for 404s (Jikan's `CACHE_404_EXPIRE`);
//! - stale-on-error plus a persisted circuit breaker on 403/429/5xx/network;
//! - no background prefetch, no per-card requests, no search fan-out.
//!
//! MAL pages are parsed directly, so Cloudflare/WAF interstitials are reported
//! as errors with a cooldown instead of being cached as empty data.

wit_bindgen::generate!({ path: "../../vendor/sdk/wit", world: "tracker-module-release-details" });

use animex::module::types::{
    AnimePreview, Character, RelatedTitle, LegacyRelease as Release, ReleaseRef, Screenshot, SimilarTitle,
    StaffMember, TitleStatistics, TrackerVideo,
};
use exports::animex::module::tracker_api::Guest;

mod html;
mod http;
mod model;
mod parse;

use model::EntryRef;

struct Component;

fn fetch_document(cache_key: &str, path: &str, ttl_ms: u64) -> Result<dom_query::Document, String> {
    let body = http::get_page(cache_key, path, ttl_ms)?;
    let body = html::trim_for(path, &body);
    http::cache_trimmed_body(cache_key, body);
    let document = html::document(body);
    Ok(document)
}

fn detail_document(entry: &EntryRef) -> Result<dom_query::Document, String> {
    let path = entry.path(None);
    fetch_document(&entry.page_key(None), &path, http::TTL_DETAIL_MS)
}

fn section_document(
    entry: &EntryRef,
    section: &str,
    ttl_ms: u64,
) -> Result<dom_query::Document, String> {
    let path = entry.path(Some(section));
    fetch_document(&entry.page_key(Some(section)), &path, ttl_ms)
}

fn fetch_characters_with_roles(id: String) -> Result<Vec<exports::animex::module::tracker_character_roles_api::CharacterWithRole>, String> {
    let entry = EntryRef::parse(&id)?;
    match section_document(&entry, "characters", http::TTL_SECTION_MS) {
        Ok(document) => Ok(parse::characters::characters_with_roles(&document)),
        Err(error) => {
            // The detail page embeds the main cast and may already be cached.
            anm_sdk::log_warn(&format!(
                "MyAnimeList: characters page for {} failed: {error}; trying detail page",
                entry.key()
            ));
            match detail_document(&entry) {
                Ok(document) => Ok(parse::characters::characters_with_roles(&document)),
                Err(_) => Err(error),
            }
        }
    }
}

impl exports::animex::module::tracker_character_roles_api::Guest for Component {
    fn get_characters(id: String) -> Result<Vec<exports::animex::module::tracker_character_roles_api::CharacterWithRole>, String> {
        fetch_characters_with_roles(id)
    }
}

impl Guest for Component {
    fn search(query: String, limit: u32) -> Result<Vec<AnimePreview>, String> {
        if query.trim().is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let path = parse::search::path(&query)?;
        let cache_key = http::search_key(&query);
        let document = fetch_document(&cache_key, &path, http::TTL_SEARCH_MS)?;
        Ok(parse::search::parse(&document, limit))
    }

    fn get_anime(id: String) -> Result<Release, String> {
        let entry = EntryRef::parse(&id)?;
        let document = detail_document(&entry)?;
        if let Some(page_id) = parse::anime::page_id(&document, entry.manga) {
            if page_id != entry.id {
                return Err("MyAnimeList: title ID mismatch".into());
            }
        }
        Ok(parse::anime::release(&document, &entry))
    }

    fn get_characters(id: String) -> Result<Vec<Character>, String> {
        Ok(fetch_characters_with_roles(id)?.into_iter().map(|entry| entry.character).collect())
    }

    fn get_staff(id: String) -> Result<Vec<StaffMember>, String> {
        let entry = EntryRef::parse(&id)?;
        if entry.manga {
            return Ok(Vec::new());
        }
        match section_document(&entry, "characters", http::TTL_SECTION_MS) {
            Ok(document) => {
                let staff = parse::characters::staff(&document);
                if staff.is_empty() {
                    return Ok(parse::characters::staff_from_detail(&document));
                }
                Ok(staff)
            }
            Err(error) => {
                anm_sdk::log_warn(&format!(
                    "MyAnimeList: staff page for {} failed: {error}; trying detail page",
                    entry.key()
                ));
                match detail_document(&entry) {
                    Ok(document) => Ok(parse::characters::staff_from_detail(&document)),
                    Err(_) => Err(error),
                }
            }
        }
    }

    fn get_videos(id: String) -> Result<Vec<TrackerVideo>, String> {
        let entry = EntryRef::parse(&id)?;
        if entry.manga {
            return Ok(Vec::new());
        }
        match section_document(&entry, "video", http::TTL_SECTION_MS) {
            Ok(document) => {
                let videos = parse::videos::videos(&document);
                if videos.is_empty() {
                    return Ok(parse::videos::videos_from_detail(&document));
                }
                Ok(videos)
            }
            Err(error) => {
                anm_sdk::log_warn(&format!(
                    "MyAnimeList: video page for {} failed: {error}; trying detail page",
                    entry.key()
                ));
                match detail_document(&entry) {
                    Ok(document) => Ok(parse::videos::videos_from_detail(&document)),
                    Err(_) => Err(error),
                }
            }
        }
    }

    fn get_screenshots(id: String) -> Result<Vec<Screenshot>, String> {
        let entry = EntryRef::parse(&id)?;
        let document = section_document(&entry, "pics", http::TTL_SECTION_MS)?;
        Ok(parse::pictures::pictures(&document))
    }
}

impl exports::animex::module::related_titles_api::Guest for Component {
    fn get_related(release: ReleaseRef) -> Result<Vec<RelatedTitle>, String> {
        let entry = EntryRef::parse(&release.id)?;
        let document = detail_document(&entry)?;
        Ok(parse::anime::related(&document))
    }
}

impl exports::animex::module::similar_titles_api::Guest for Component {
    fn get_similar(release: ReleaseRef) -> Result<Vec<SimilarTitle>, String> {
        let entry = EntryRef::parse(&release.id)?;
        match section_document(&entry, "userrecs", http::TTL_SECTION_MS) {
            Ok(document) => {
                let similar = parse::recommendations::recommendations(&document, entry.manga, 50);
                if similar.is_empty() {
                    return Ok(parse::recommendations::recommendations_from_detail(
                        &document,
                        entry.manga,
                        50,
                    ));
                }
                Ok(similar)
            }
            Err(error) => {
                anm_sdk::log_warn(&format!(
                    "MyAnimeList: recommendations for {} failed: {error}; trying detail page",
                    entry.key()
                ));
                match detail_document(&entry) {
                    Ok(document) => Ok(parse::recommendations::recommendations_from_detail(
                        &document,
                        entry.manga,
                        50,
                    )),
                    Err(_) => Err(error),
                }
            }
        }
    }
}

impl exports::animex::module::statistics_api::Guest for Component {
    fn get_statistics(release: ReleaseRef) -> Result<TitleStatistics, String> {
        let entry = EntryRef::parse(&release.id)?;
        match section_document(&entry, "stats", http::TTL_SECTION_MS) {
            Ok(document) => Ok(parse::statistics::statistics(&document, entry.manga)),
            Err(error) => {
                // Rating is also present in the detail sidebar. Returning the
                // rating without distributions is better than losing the whole
                // statistics tab when only /stats is unavailable.
                anm_sdk::log_warn(&format!(
                    "MyAnimeList: statistics for {} failed: {error}; using detail rating",
                    entry.key()
                ));
                match detail_document(&entry) {
                    Ok(document) => Ok(TitleStatistics {
                        rating: parse::anime::rating(&document),
                        score_scale: 10,
                        scores: Vec::new(),
                        statuses: Vec::new(),
                    }),
                    Err(_) => Err(error),
                }
            }
        }
    }
}

anm_sdk::release_metadata!();

impl exports::animex::module::release_details_api::Guest for Component {
    fn get_release_details(reference: ReleaseRef) -> Result<crate::animex::module::types::Release, String> {

        let entry = EntryRef::parse(&reference.id)?;
        let document = detail_document(&entry)?;
        if parse::anime::page_id(&document, entry.manga).is_some_and(|id| id != entry.id) {
            return Err("MyAnimeList: title ID mismatch".into());
        }
        Ok(parse::anime::extended_release(&document, &entry))
    }
}

export!(Component);

impl exports::animex::module::tracker_episodes_api::Guest for Component {
    fn get_episodes(
        id: String,
        page: u32,
    ) -> Result<animex::module::types::TrackerEpisodePage, String> {
        let entry = EntryRef::parse(&id)?;
        let (cache_key, path) = parse::episodes::request(&entry, page)?;
        if entry.manga {
            return Ok(parse::episodes::empty_page(page));
        }
        let document = fetch_document(&cache_key, &path, http::TTL_SECTION_MS)?;
        parse::episodes::parse(&document, &entry, page)
    }
}
