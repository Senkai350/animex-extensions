wit_bindgen::generate!({
    path: "../../vendor/sdk/wit",
    world: "source-module-related",
});

use animex::module::types::{
    AiringStatus, AnimePreview, CatalogPage, CatalogParams, EpisodeRef, PageMeta, PlayerCandidate,
    Release, ReleaseRef, TrackerData,
};
use exports::animex::module::source_api::Guest;

struct Component;

impl Guest for Component {
    fn get_filters() -> Result<Vec<animex::module::types::FilterField>, String> {
        Ok(vec![animex::module::types::FilterField::Text(
            animex::module::types::FilterText {
                id: "q".into(),
                label: "Поиск".into(),
                default: None,
            },
        )])
    }

    fn get_catalog(_params: CatalogParams) -> Result<CatalogPage, String> {
        Ok(page(1))
    }

    fn get_latest(page_num: u32) -> Result<CatalogPage, String> {
        Ok(page(page_num))
    }

    fn get_popular(page_num: u32) -> Result<CatalogPage, String> {
        Ok(page(page_num))
    }

    fn search(query: String, _params: CatalogParams) -> Result<CatalogPage, String> {
        Ok(page_with_title(&query))
    }

    fn get_release(release: ReleaseRef) -> Result<Release, String> {
        Ok(Release {
            id: release.id.clone(),
            name: format!("Dummy {}", release.id),
            alternative_names: vec![],
            tracker_data: vec![],
            other_info: vec![],
            banner_url: None,
            poster: None,
            description: Some("dummy".into()),
            genres: vec!["Test".into()],
            studios: vec![],
            status: AiringStatus::Ongoing,
            kind: Some("TV".into()),
            year: Some(2026),
            trailer: None,
            total_episodes: Some(12),
            next_episode_at: None,
            rating: Some(7.5),
            source_ref: None,
        })
    }

    fn get_episodes(_release: ReleaseRef) -> Result<Vec<animex::module::types::Episode>, String> {
        Ok(vec![])
    }

    fn get_players(
        _release: ReleaseRef,
        episode: EpisodeRef,
    ) -> Result<Vec<PlayerCandidate>, String> {
        Ok(vec![PlayerCandidate {
            protocol: "dummy.echo.v1".into(),
            payload: format!(r#"{{"url":"https://example.com/{}.m3u8"}}"#, episode.id),
            label: Some("Dummy".into()),
            priority: Some(0),
        }])
    }
}

fn preview(id: &str) -> AnimePreview {
    AnimePreview {
        id: id.to_string(),
        title: format!("Dummy {id}"),
        alternative_names: vec![],
        genres: vec!["Test".into()],
        description: Some("dummy".into()),
        poster: None,
        year: Some(2026),
        score: Some(7.5),
        kind: Some("TV".into()),
        status: AiringStatus::Ongoing,
        episodes_count: Some(12),
        tracker_data: vec![TrackerData {
            name: "Shikimori".into(),
            data: "1".into(),
        }],
        source_ref: None,
    }
}

fn page(n: u32) -> CatalogPage {
    CatalogPage {
        items: vec![preview("a"), preview("b")],
        meta: PageMeta {
            page: n,
            per_page: 2,
            total: Some(2),
            total_pages: Some(1),
            has_next: false,
        },
    }
}

fn page_with_title(title: &str) -> CatalogPage {
    let mut p = page(1);
    p.items = vec![AnimePreview {
        title: title.to_string(),
        ..preview("search")
    }];
    p
}

impl exports::animex::module::related_titles_api::Guest for Component {
    fn get_related(
        release: ReleaseRef,
    ) -> Result<Vec<animex::module::types::RelatedTitle>, String> {
        use animex::module::types::{MediaType, RelatedTitle, TitleCard, TitleRelation};
        if release.id == "empty" {
            return Ok(vec![]);
        }
        if release.id == "error" {
            return Err("dummy related failure".into());
        }
        Ok(vec![RelatedTitle {
            title: TitleCard {
                id: "related".into(),
                name: "Dummy related".into(),
                alternative_names: vec![],
                poster: None,
                media_type: MediaType::Anime,
                format: Some("tv".into()),
                status: AiringStatus::Released,
                year: Some(2026),
                tracker_data: vec![],
                source_ref: release.payload,
                url: None,
            },
            relation: TitleRelation {
                code: "prequel".into(),
                label: "Предыстория".into(),
            },
        }])
    }
}

export!(Component);
