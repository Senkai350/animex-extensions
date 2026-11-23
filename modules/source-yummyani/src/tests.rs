//! Offline tests over sanitized API captures. Real embed tokens never appear
//! here: Alloha and Aksor URLs use placeholder values of the same shape, and
//! descriptions are shortened snippets.

use super::*;

const TEST_BASE: &str = "https://old.yummyani.me";

// --- helpers -------------------------------------------------------------------

fn catalog_item(index: usize) -> serde_json::Value {
    serde_json::json!({
        "anime_id": index,
        "title": format!("Пример {index}"),
        "description": "Короткое описание.",
        "year": 2026,
        "poster": {"medium": format!("//imgproxy.yani.tv/posters/medium/{index}.webp")},
        "genres": [{"title": "Фэнтези"}],
        "anime_status": {"value": 1, "title": "онгоинг", "alias": "ongoing"},
        "type": {"name": "Сериал", "value": 1, "shortname": "ТВ", "alias": "tv"},
        "rating": {"average": 8.0},
        "remote_ids": {"shikimori_id": 59787, "myanimelist_id": 59787}
    })
}

fn catalog_body(count: usize) -> String {
    let items: Vec<_> = (1..=count).map(catalog_item).collect();
    serde_json::json!({"response": items}).to_string()
}

fn row(
    player_id: u32,
    dubbing: &str,
    number: serde_json::Value,
    iframe_url: &str,
    duration: u64,
    opening: Option<(u32, u32)>,
    ending: Option<(u32, u32)>,
) -> serde_json::Value {
    let opening = opening
        .map(|(time, length)| serde_json::json!({"time": time, "length": length}))
        .unwrap_or(serde_json::Value::Null);
    let ending = ending
        .map(|(time, length)| serde_json::json!({"time": time, "length": length}))
        .unwrap_or(serde_json::Value::Null);
    serde_json::json!({
        "video_id": 1330000 + u64::from(player_id),
        "data": {"player": "Плеер", "dubbing": dubbing, "player_id": player_id},
        "number": number,
        "iframe_url": iframe_url,
        "duration": duration,
        "skips": {"opening": opening, "ending": ending},
        "views": 100
    })
}

fn rows_body(rows: &[serde_json::Value]) -> String {
    serde_json::json!({"response": rows}).to_string()
}

fn decode_rows(body: &str) -> Vec<VideoRow> {
    decode(body, "videos").unwrap()
}

// --- pagination ----------------------------------------------------------------

#[test]
fn catalog_urls_use_the_probe_limit_offset_and_status() {
    assert_eq!(
        catalog_url_for(TEST_BASE, 1, None, None).unwrap(),
        "https://old.yummyani.me/api/anime?limit=21&offset=0"
    );
    assert_eq!(
        catalog_url_for(TEST_BASE, 3, Some("year"), Some("ongoing")).unwrap(),
        "https://old.yummyani.me/api/anime?limit=21&offset=40&sort=year&status=ongoing"
    );
    let url = Url::parse(&search_url_for(TEST_BASE, "ромелия", 2).unwrap()).unwrap();
    assert_eq!(url.path(), "/api/search");
    let query: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert!(query.contains(&("q".to_string(), "ромелия".to_string())));
    assert!(query.contains(&("limit".to_string(), "21".to_string())));
    assert!(query.contains(&("offset".to_string(), "20".to_string())));
}

#[test]
fn listing_pages_probe_the_next_page_with_one_extra_row() {
    let page = listing_page(&catalog_body(21), 1).unwrap();
    assert_eq!(page.items.len(), 20);
    assert!(page.meta.has_next);
    assert_eq!(page.items[0].id, "1");
    assert_eq!(page.items[19].id, "20");

    let page = listing_page(&catalog_body(20), 2).unwrap();
    assert_eq!(page.items.len(), 20);
    assert!(!page.meta.has_next);
    assert_eq!(page.meta.page, 2);

    let page = listing_page(&catalog_body(7), 1).unwrap();
    assert_eq!(page.items.len(), 7);
    assert!(!page.meta.has_next);

    let empty = listing_page(r#"{"response":[]}"#, 1).unwrap();
    assert!(empty.items.is_empty());
    assert!(!empty.meta.has_next);
}

#[test]
fn listing_pages_show_only_the_first_twenty_raw_rows() {
    // An invalid row among the first 20 must not pull the probe row in:
    // the 21st raw row only proves `has_next` and is never shown.
    let mut items: Vec<_> = (1..=21).map(catalog_item).collect();
    items[4]["title"] = serde_json::json!("   ");
    let page = listing_page(&serde_json::json!({"response": items}).to_string(), 1).unwrap();
    assert_eq!(page.items.len(), 19);
    assert!(page.meta.has_next);
    assert!(
        page.items.iter().all(|preview| preview.id != "21"),
        "the probe row must never be shown: {:?}",
        page.items
            .iter()
            .map(|preview| preview.id.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn error_envelopes_surface_the_site_reason() {
    let body = r#"{"error":"Value -year is not in enum [\"title\",\"year\",\"rating\"]","error_title":"Ошибка","error_code":3}"#;
    let error = decode::<Vec<AnimeItem>>(body, "catalog").unwrap_err();
    assert!(error.contains("-year"), "{error}");
}

// --- sorting, filters and reserved feeds ----------------------------------------

#[test]
fn only_real_ascending_sorts_are_accepted() {
    let params = |sort: Option<String>| CatalogParams {
        page: 1,
        search: None,
        sort,
        filters: vec![],
    };
    assert_eq!(
        sort_of(&params(Some("views".into()))),
        Ok(Some("views".into()))
    );
    assert_eq!(
        sort_of(&params(Some(serde_json::json!("rating").to_string()))),
        Ok(Some("rating".into()))
    );
    assert_eq!(
        sort_of(&params(Some(
            serde_json::json!({"value": "year"}).to_string()
        ))),
        Ok(Some("year".into()))
    );
    assert_eq!(sort_of(&params(None)), Ok(None));
    // Unknown selections fail without echoing the raw value.
    let error = sort_of(&params(Some("SECRETSORT".into()))).unwrap_err();
    assert!(!error.contains("SECRETSORT"), "{error}");
    assert!(sort_of(&params(Some("-rating".into()))).is_err());

    let sorting = Component::get_sorting().unwrap().unwrap();
    assert_eq!(sorting.id, "sort");
    assert_eq!(sorting.options.len(), 5);
    assert!(sorting.directions.is_empty());
}

#[test]
fn only_validated_status_aliases_are_offered_and_sent() {
    let params = |status: Option<&str>| CatalogParams {
        page: 1,
        search: None,
        sort: None,
        filters: status
            .map(|status| vec![("status".to_string(), status.to_string())])
            .unwrap_or_default(),
    };
    assert_eq!(status_filter_of(&params(None)), Ok(None));
    assert_eq!(
        status_filter_of(&params(Some("ongoing"))),
        Ok(Some("ongoing".into()))
    );
    assert_eq!(
        status_filter_of(&params(Some(
            serde_json::json!({"value": "released"})
                .to_string()
                .as_str()
        ))),
        Ok(Some("released".into()))
    );
    let error = status_filter_of(&params(Some("SECRETSTATUS"))).unwrap_err();
    assert!(!error.contains("SECRETSTATUS"), "{error}");

    let filters = Component::get_filters().unwrap();
    assert_eq!(filters.len(), 1);
    match &filters[0] {
        FilterField::Select(select) => {
            assert_eq!(select.id, "status");
            let values: Vec<&str> = select
                .options
                .iter()
                .map(|option| option.value.as_str())
                .collect();
            assert_eq!(values, vec!["ongoing", "released", "announcement"]);
        }
        _ => panic!("expected a select filter"),
    }
    let ui = Component::get_filter_ui().unwrap();
    assert_eq!(ui.len(), 1);
    assert_eq!(ui[0].id, "status");
}

#[test]
fn reserved_feeds_report_successful_empty_pages() {
    // The site has no updates feed and no verified descending views
    // ordering: an empty page represents the unsupported operation instead
    // of a global home error or mislabelled ascending rows.
    let latest = Component::get_latest(1).unwrap();
    assert!(latest.items.is_empty());
    assert!(!latest.meta.has_next);
    let popular = Component::get_popular(2).unwrap();
    assert!(popular.items.is_empty());
    assert!(!popular.meta.has_next);
    assert_eq!(popular.meta.page, 2);
}

// --- release -------------------------------------------------------------------

const RELEASE_BODY: &str = r#"{"response":{"anime_id":15921,"title":"Военная хроника Ромелии","description":"Короткое описание выпуска.","poster":{"fullsize":"//imgproxy.yani.tv/posters/full/x.jpg","big":"//imgproxy.yani.tv/posters/big/x.webp","small":"//imgproxy.yani.tv/posters/small/x.webp","medium":"//imgproxy.yani.tv/posters/medium/x.webp"},"rating":{"average":8.41},"genres":[{"title":"Фэнтези"},{"title":"Экшен"}],"studios":[{"title":"Atra"}],"year":2026,"anime_status":{"value":1,"title":"онгоинг","alias":"ongoing"},"type":{"name":"Сериал","value":1,"shortname":"ТВ","alias":"tv"},"remote_ids":{"worldart_id":12685,"shikimori_id":59787,"myanimelist_id":59787,"kp_id":0},"other_titles":["Romelia Senki","ロメリア戦記","Воєнна хроніка Ромелії"],"episodes":{"count":12,"aired":7,"next_date":1795444200},"min_age":{"value":3,"title_long":"PG-13 (от 13 лет)"},"original":"Ранобэ"}}"#;

#[test]
fn release_maps_real_fields_and_real_tracker_ids() {
    let item: AnimeItem = decode(RELEASE_BODY, "release").unwrap();
    let release = map_release(&item, "15921").unwrap();
    assert_eq!(release.id, "15921");
    assert_eq!(release.name, "Военная хроника Ромелии");
    assert_eq!(release.kind.as_deref(), Some("tv"));
    assert_eq!(release.status, AiringStatus::Ongoing);
    assert_eq!(release.year, Some(2026));
    assert_eq!(release.total_episodes, Some(12));
    assert_eq!(
        release.next_episode_at.as_deref(),
        Some("2026-11-23T14:30:00Z")
    );
    assert_eq!(release.rating, Some(8.41));
    assert_eq!(release.genres, vec!["Фэнтези", "Экшен"]);
    assert_eq!(release.studios, vec!["Atra"]);
    assert_eq!(
        release.poster.as_deref(),
        Some("https://imgproxy.yani.tv/posters/medium/x.webp")
    );
    assert_eq!(release.banner_url, release.poster);
    let values: Vec<(&str, &str)> = release
        .tracker_data
        .iter()
        .map(|item| (item.name.as_str(), item.data.as_str()))
        .collect();
    assert_eq!(
        values,
        vec![("Shikimori", "59787"), ("MyAnimeList", "59787")]
    );
    assert!(release.source_ref.is_none());
    let titles: Vec<(&str, &str)> = release
        .alternative_names
        .iter()
        .map(|name| (name.name.as_str(), name.language.as_str()))
        .collect();
    assert_eq!(
        titles,
        vec![
            ("Romelia Senki", "unknown"),
            ("ロメリア戦記", "ja"),
            ("Воєнна хроніка Ромелії", "ru"),
        ]
    );
    for label in ["Тип", "Статус", "Возраст", "Первоисточник"] {
        assert!(
            release.other_info.iter().any(|block| block.title == label),
            "missing {label}"
        );
    }
}

#[test]
fn zero_remote_ids_produce_no_tracker_data() {
    let item: AnimeItem = decode(
        r#"{"response":{"title":"Пример","remote_ids":{"shikimori_id":0,"myanimelist_id":0}}}"#,
        "release",
    )
    .unwrap();
    let release = map_release(&item, "1").unwrap();
    assert!(release.tracker_data.is_empty());
    assert_eq!(release.rating, None);
}

#[test]
fn statuses_come_from_real_site_aliases() {
    let status = |body: &str| -> AiringStatus {
        let item: AnimeItem = decode(body, "release").unwrap();
        status_of(&item)
    };
    assert_eq!(
        status(r#"{"response":{"anime_status":{"alias":"ongoing"}}}"#),
        AiringStatus::Ongoing
    );
    assert_eq!(
        status(r#"{"response":{"anime_status":{"alias":"released"}}}"#),
        AiringStatus::Released
    );
    assert_eq!(
        status(r#"{"response":{"anime_status":{"alias":"announcement"}}}"#),
        AiringStatus::Anons
    );
    assert_eq!(
        status(r#"{"response":{"anime_status":{"value":2,"alias":"announcement"}}}"#),
        AiringStatus::Anons
    );
    assert_eq!(
        status(r#"{"response":{"anime_status":{"value":0}}}"#),
        AiringStatus::Released
    );
    assert_eq!(status(r#"{"response":{}}"#), AiringStatus::Unknown);
}

#[test]
fn release_ids_must_be_numeric_site_ids() {
    assert_eq!(numeric_release_id("15921").unwrap(), "15921");
    assert!(numeric_release_id("voennaya-hronika-romelii").is_err());
    assert!(numeric_release_id("").is_err());
}

// --- episodes ------------------------------------------------------------------

#[test]
fn episode_ids_must_be_canonical_for_the_requested_release() {
    assert_eq!(episode_number_of("15921", "15921-7").unwrap(), 7);
    // Bare numbers, other releases and non-canonical forms are rejected.
    assert!(episode_number_of("15921", "7").is_err());
    assert!(episode_number_of("15921", "15999-7").is_err());
    assert!(episode_number_of("15921", "15921-x").is_err());
    assert!(episode_number_of("15921", "15921-0").is_err());
    assert!(episode_number_of("15921", "15921-7-7").is_err());
}

#[test]
fn episodes_dedup_sort_numerically_and_carry_skips() {
    let rows: Vec<VideoRow> = decode_rows(&rows_body(&[
        row(
            3,
            "Субтитры Манипулятор",
            serde_json::json!("2"),
            "//ru.yummyani.me/iframeCVH.html?dubbing_code=X&anime_id=1&episode=2",
            0,
            None,
            None,
        ),
        row(
            4,
            "Озвучка AniLibria",
            serde_json::json!(1),
            "https://kodikplayer.com/season/94795/d5a209e95d68094e346175d7357ef0c7/720p?episode=1",
            1420,
            Some((244, 89)),
            None,
        ),
        row(
            2,
            "Озвучка AniLibria",
            serde_json::json!("10"),
            "//alloha.yani.tv/?token_movie=0&translation=1&season=1&episode=10&token=1",
            1421,
            None,
            Some((1400, 19)),
        ),
        row(
            4,
            "Озвучка AniStar",
            serde_json::json!("1"),
            "https://kodikplayer.com/season/1/abc/720p?episode=1",
            1440,
            Some((250, 89)),
            None,
        ),
    ]));
    let episodes = episodes_from_rows(&rows, "15921");
    let numbers: Vec<u32> = episodes.iter().map(|episode| episode.number).collect();
    assert_eq!(numbers, vec![1, 2, 10]);
    assert_eq!(episodes[0].id, "15921-1");
    assert_eq!(episodes[0].duration, Some(1440.0));
    let skips: Vec<(f32, f32)> = episodes[0]
        .time_skips
        .iter()
        .map(|skip| (skip.start, skip.end))
        .collect();
    assert_eq!(skips, vec![(244.0, 333.0)]);
    assert!(episodes[0].source_ref.is_none());
    assert_eq!(episodes[1].duration, None);
    let ending: Vec<(f32, f32)> = episodes[2]
        .time_skips
        .iter()
        .map(|skip| (skip.start, skip.end))
        .collect();
    assert_eq!(ending, vec![(1400.0, 1419.0)]);
}

// --- player routing --------------------------------------------------------------

const CVH_ANILIBRIA: &str = "//ru.yummyani.me/iframeCVH.html?dubbing_code=AniLibria&anime_id=52991&episode=1&dubbing=%D0%9E%D0%B7%D0%B2%D1%83%D1%87%D0%BA%D0%B0";
const CVH_MANIPULATOR: &str = "//ru.yummyani.me/iframeCVH.html?dubbing_code=%D0%9C%D0%B0%D0%BD%D0%B8%D0%BF%D1%83%D0%BB%D1%8F%D1%82%D0%BE%D1%80&anime_id=59787&episode=1&dubbing=%D0%A1%D1%83%D0%B1%D1%82%D0%B8%D1%82%D1%80%D1%8B";
const KODIK_FLOWERS: &str = "//kodikplayer.com/season/94795/d5a209e95d68094e346175d7357ef0c7/720p?translations=false&only_episode=true&only_season=true&episode=1";
const ALLOHA_ANILIBRIA: &str = "//alloha.yani.tv/?token_movie=00000000000000000000000000000000&translation=340&season=1&episode=1&token=111111111111111111111111111111&hidden=translation,season,episode";
const AKSOR_ANILIBRIA: &str = "https://player.aksor.tv/video/0123456789abcdef0123456789abcdef";

fn episode_rows() -> Vec<serde_json::Value> {
    vec![
        row(
            4,
            "Озвучка Flowers Media",
            serde_json::json!("1"),
            KODIK_FLOWERS,
            1420,
            Some((250, 89)),
            Some((1400, 19)),
        ),
        row(
            3,
            "Озвучка AniLibria",
            serde_json::json!("1"),
            CVH_ANILIBRIA,
            1420,
            None,
            None,
        ),
        row(
            2,
            "Озвучка AniLibria",
            serde_json::json!("1"),
            ALLOHA_ANILIBRIA,
            1420,
            Some((244, 89)),
            None,
        ),
        row(
            3,
            "Субтитры Манипулятор",
            serde_json::json!("1"),
            CVH_MANIPULATOR,
            1420,
            None,
            None,
        ),
        row(
            1,
            "Озвучка AniLibria",
            serde_json::json!(1),
            AKSOR_ANILIBRIA,
            1560,
            None,
            None,
        ),
    ]
}

#[test]
fn players_route_exactly_by_player_id_and_voice() {
    let rows = decode_rows(&rows_body(&episode_rows()));
    let scoped: Vec<&VideoRow> = rows.iter().collect();
    let candidates = player_candidates(&scoped);
    assert_eq!(candidates.len(), 4, "{candidates:?}");
    assert_eq!(candidates[0].protocol, "cvh.embed.v1");
    assert_eq!(candidates[1].protocol, "kodik.embed.v1");
    assert_eq!(candidates[2].protocol, "alloha.embed.v1");
    assert_eq!(candidates[3].protocol, "aksor.embed.v1");
    for (index, candidate) in candidates.iter().enumerate() {
        assert_eq!(candidate.priority, Some(index as u32));
    }
    assert_eq!(candidates[0].label.as_deref(), Some("CVH"));
    assert_eq!(candidates[1].label.as_deref(), Some("Kodik"));
    assert_eq!(candidates[2].label.as_deref(), Some("Alloha"));
    assert_eq!(candidates[3].label.as_deref(), Some("Aksor"));

    let cvh: EmbedPayload = serde_json::from_str(&candidates[0].payload).unwrap();
    assert_eq!(cvh.referer.as_deref(), Some("https://ru.yummyani.me/"));
    assert_eq!(cvh.variants.len(), 2);
    assert_eq!(cvh.variants[0].id, "Озвучка AniLibria");
    assert_eq!(cvh.variants[1].id, "Субтитры Манипулятор");
    let context = cvh.variants[0].cvh.as_ref().unwrap();
    assert_eq!(context.media_id, "52991");
    assert_eq!(context.voice, "AniLibria");
    assert_eq!(context.episode, 1);
    assert_eq!(context.season, 1);
    assert_eq!(context.publisher_id, "745");
    assert_eq!(context.aggregator, "mali");
    let context = cvh.variants[1].cvh.as_ref().unwrap();
    assert_eq!(context.media_id, "59787");
    assert_eq!(context.voice, "Манипулятор");
    assert_eq!(
        cvh.variants[0].url,
        "https://ru.yummyani.me/iframeCVH.html?dubbing_code=AniLibria&anime_id=52991&episode=1&dubbing=%D0%9E%D0%B7%D0%B2%D1%83%D1%87%D0%BA%D0%B0"
    );

    let kodik: EmbedPayload = serde_json::from_str(&candidates[1].payload).unwrap();
    assert_eq!(kodik.referer.as_deref(), Some("https://old.yummyani.me/"));
    assert_eq!(kodik.variants.len(), 1);
    assert_eq!(kodik.variants[0].id, "Озвучка Flowers Media");
    assert_eq!(
        kodik.variants[0].url,
        "https://kodikplayer.com/season/94795/d5a209e95d68094e346175d7357ef0c7/720p?translations=false&only_episode=true&only_season=true&episode=1"
    );

    let alloha: EmbedPayload = serde_json::from_str(&candidates[2].payload).unwrap();
    assert_eq!(alloha.variants.len(), 1);
    assert_eq!(alloha.variants[0].id, "Озвучка AniLibria");
    assert_eq!(
        alloha.variants[0].url,
        "https://alloha.yani.tv/?token_movie=00000000000000000000000000000000&translation=340&season=1&episode=1&token=111111111111111111111111111111&hidden=translation,season,episode"
    );

    let aksor: EmbedPayload = serde_json::from_str(&candidates[3].payload).unwrap();
    assert_eq!(aksor.variants.len(), 1);
    assert_eq!(aksor.variants[0].url, AKSOR_ANILIBRIA);
    assert!(aksor.variants[0].cvh.is_none());
}

#[test]
fn players_skip_mismatched_hosts_unknown_players_and_plain_http() {
    let rows = decode_rows(&rows_body(&[
        row(
            3,
            "Озвучка AniLibria",
            serde_json::json!("1"),
            CVH_ANILIBRIA,
            1420,
            None,
            None,
        ),
        // Alloha player id on a foreign host.
        row(
            2,
            "Озвучка X",
            serde_json::json!("1"),
            AKSOR_ANILIBRIA,
            1420,
            None,
            None,
        ),
        // Unknown player id.
        row(
            9,
            "Озвучка Y",
            serde_json::json!("1"),
            KODIK_FLOWERS,
            1420,
            None,
            None,
        ),
        // Aksor id with a wrong path.
        row(
            1,
            "Озвучка Z",
            serde_json::json!("1"),
            "https://player.aksor.tv/video/short",
            1420,
            None,
            None,
        ),
        // Alloha id without the token parameters.
        row(
            2,
            "Озвучка W",
            serde_json::json!("1"),
            "https://alloha.yani.tv/?translation=1",
            1420,
            None,
            None,
        ),
        // Plain http embeds are not accepted.
        row(
            4,
            "Озвучка V",
            serde_json::json!("1"),
            "http://kodikplayer.com/season/1/abc/720p",
            1420,
            None,
            None,
        ),
        // CVH id on a foreign path.
        row(
            3,
            "Озвучка U",
            serde_json::json!("1"),
            "https://ru.yummyani.me/other.html?anime_id=1&dubbing_code=A&episode=1",
            1420,
            None,
            None,
        ),
    ]));
    let scoped: Vec<&VideoRow> = rows.iter().collect();
    let candidates = player_candidates(&scoped);
    assert_eq!(candidates.len(), 1, "{candidates:?}");
    assert_eq!(candidates[0].protocol, "cvh.embed.v1");
    let payload: EmbedPayload = serde_json::from_str(&candidates[0].payload).unwrap();
    assert_eq!(payload.variants.len(), 1);
    assert_eq!(payload.variants[0].id, "Озвучка AniLibria");
}

#[test]
fn cvh_rows_require_the_exact_episode_and_an_unambiguous_context() {
    // The iframe names episode 1 while the row is episode 2: skipped, no
    // substitution.
    let rows = decode_rows(&rows_body(&[
        row(3, "Озвучка AniLibria", serde_json::json!("2"), "//ru.yummyani.me/iframeCVH.html?dubbing_code=AniLibria&anime_id=52991&episode=1", 1420, None, None),
        // A blank dubbing_code leaves the voice ambiguous.
        row(3, "Озвучка StudioBand", serde_json::json!("1"), "//ru.yummyani.me/iframeCVH.html?dubbing_code=&anime_id=52991&episode=1", 1420, None, None),
        // Repeated anime_id parameters are ambiguous.
        row(3, "Озвучка Dream Cast", serde_json::json!("1"), "//ru.yummyani.me/iframeCVH.html?dubbing_code=Dream&anime_id=52991&anime_id=59787&episode=1", 1420, None, None),
        // The exact episode with a clean context stays routable.
        row(3, "Озвучка AniStar", serde_json::json!("2"), "//ru.yummyani.me/iframeCVH.html?dubbing_code=AniStar&anime_id=52991&episode=2", 1420, None, None),
    ]));
    let scoped: Vec<&VideoRow> = rows.iter().collect();
    let candidates = player_candidates(&scoped);
    assert_eq!(candidates.len(), 1, "{candidates:?}");
    let payload: EmbedPayload = serde_json::from_str(&candidates[0].payload).unwrap();
    assert_eq!(payload.variants.len(), 1);
    assert_eq!(payload.variants[0].id, "Озвучка AniStar");
    let context = payload.variants[0].cvh.as_ref().unwrap();
    assert_eq!(context.episode, 2);
    assert_eq!(context.voice, "AniStar");
}

#[test]
fn alloha_rows_require_complete_unique_params_and_the_exact_episode() {
    let rows = decode_rows(&rows_body(&[
        // Numeric episode 1 for a row of episode 2: skipped.
        row(2, "Озвучка A", serde_json::json!("2"), "//alloha.yani.tv/?token_movie=00000000000000000000000000000000&translation=340&season=1&episode=1&token=111111111111111111111111111111", 1420, None, None),
        // Blank required parameter.
        row(2, "Озвучка B", serde_json::json!("2"), "//alloha.yani.tv/?token_movie=&translation=340&season=1&episode=2&token=111111111111111111111111111111", 1420, None, None),
        // Duplicate required parameter.
        row(2, "Озвучка C", serde_json::json!("2"), "//alloha.yani.tv/?token_movie=00000000000000000000000000000000&translation=340&season=1&episode=2&token=1&token=2", 1420, None, None),
        // Missing parameter.
        row(2, "Озвучка D", serde_json::json!("2"), "//alloha.yani.tv/?translation=340&season=1&episode=2&token=1", 1420, None, None),
        // The exact episode with complete parameters stays routable.
        row(2, "Озвучка E", serde_json::json!("2"), &ALLOHA_ANILIBRIA.replace("episode=1", "episode=2"), 1420, None, None),
    ]));
    let scoped: Vec<&VideoRow> = rows.iter().collect();
    let candidates = player_candidates(&scoped);
    assert_eq!(candidates.len(), 1, "{candidates:?}");
    let payload: EmbedPayload = serde_json::from_str(&candidates[0].payload).unwrap();
    assert_eq!(payload.variants.len(), 1);
    assert_eq!(payload.variants[0].id, "Озвучка E");
}

#[test]
fn video_numbers_accept_strings_and_numbers() {
    let rows = decode_rows(&rows_body(&[
        row(
            1,
            "Озвучка A",
            serde_json::json!("1"),
            AKSOR_ANILIBRIA,
            10,
            None,
            None,
        ),
        row(
            1,
            "Озвучка A",
            serde_json::json!(2),
            AKSOR_ANILIBRIA,
            10,
            None,
            None,
        ),
        row(
            1,
            "Озвучка A",
            serde_json::json!("oops"),
            AKSOR_ANILIBRIA,
            10,
            None,
            None,
        ),
        row(
            1,
            "Озвучка A",
            serde_json::Value::Null,
            AKSOR_ANILIBRIA,
            10,
            None,
            None,
        ),
    ]));
    let numbers: Vec<u32> = rows.iter().filter_map(row_episode_number).collect();
    assert_eq!(numbers, vec![1, 2]);
}

// --- timestamps ----------------------------------------------------------------

#[test]
fn rfc3339_timestamps_match_utc() {
    assert_eq!(rfc3339_utc(0).as_deref(), Some("1970-01-01T00:00:00Z"));
    assert_eq!(
        rfc3339_utc(951782400).as_deref(),
        Some("2000-02-29T00:00:00Z")
    );
    assert_eq!(
        rfc3339_utc(1795444200).as_deref(),
        Some("2026-11-23T14:30:00Z")
    );
}
