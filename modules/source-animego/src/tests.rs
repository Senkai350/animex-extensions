//! Offline tests: real AnimeGO markup fragments captured from live pages.

use super::*;

const CARD_HTML: &str = r#"
<div class="scroll-snap-slide ani-grid__item position-relative align-items-start">
    <div class="rating-badge positive">8.6</div>
    <a class="d-block ani-grid__item-picture image__picture aspect-ratio-5-7 w-100" href="/anime/svad-ba-svetlyachkov-3815">
        <img class="image__img rounded" src="https://img.cdngos.com/v/250x350/anime/6a/6acd3a0f08d76086570449" alt="Свадьба светлячков">
    </a>
    <div class="ani-grid__item-body pt-2 text-break">
        <div class="fw-lighter small text-line-clamp">Hotaru no Yomeiri</div>
        <div class="ani-grid__item-title h5 fw-normal mb-1">
            <a class="text-decoration-none text-line-clamp" title="Свадьба светлячков" href="/anime/svad-ba-svetlyachkov-3815">Свадьба светлячков</a>
        </div>
    </div>
</div>
"#;

const VOICE_BUTTONS_HTML: &str = r#"
<div id="provider">
    <button data-ptranslation="610" data-provider-title="AniBoom" data-player="//aniboom.one/embed/9G1MJ6NMV8z?episode=1&translation=610" data-translation-title="AniLibria»(* Theme225;)  (ошибка)"></button>
    <button data-ptranslation="609" data-provider-title="AniBoom" data-player="//aniboom.one/embed/AAAA1111?episode=1&translation=609" data-translation-title="AniDUB"></button>
    <button data-ptranslation="801" data-provider-title="CVH" data-player="https://animego.me/cdn-iframe/51019/1/1?dubbing=AnilibriaTV" data-translation-title="AnilibriaTV"></button>
    <button data-provider-title="Sibnet" data-player="//video.sibnet.ru/shell.php?videoid=123" data-translation-title=""></button>
    <button data-ptranslation="802" data-provider-title="Kodik" data-player="https://kodikplayer.com/serial/1304528/abc123/720p" data-translation-title="Студия"></button>
</div>
"#;

const EPISODE_PAGES_HTML: &str = r#"
<div class="episode-block">
    <a class="link-square" data-episode="773273" data-episode-number="1">1</a>
    <a class="link-square" data-episode="773274" data-episode-number="2">2</a>
    <a class="link-square" data-episode="773275" data-episode-number="3">3</a>
</div>
"#;

const SCHEDULE_HTML: &str = r#"
<div class="row m-0">
    <div class="col-1"><meta content="1"></div>
    <div class="col-5">Начало пути</div>
    <div class="col-3"><span data-label="02.10.2026">02.10.2026</span></div>
    <div class="col-3"><span data-watched-id="991">1</span></div>
</div>
<div class="row m-0">
    <div class="col-1"><meta content="2"></div>
    <div class="col-5">---</div>
    <div class="col-3"><span data-label="09.10.2026">09.10.2026</span></div>
    <div class="col-3"><span data-watched-id="992">2</span></div>
</div>
<div class="row m-0">
    <div class="col-1"><meta content="3"></div>
    <div class="col-5"></div>
    <div class="col-3"><span data-label="16.10.2026">16.10.2026</span></div>
    <div class="col-3"></div>
</div>
"#;

const ENTITY_HTML: &str = r#"
<div class="entity">
    <div class="entity__row">
        <h1>Свадьба светлячков</h1>
        <div class="entity__title-synonyms">Hotaru no Yomeiri / Кладбище светлячков</div>
    </div>
    <div class="entity__rating"><span class="entity-rating__value">8.6</span></div>
    <div class="entity-field">
        <div>Тип</div>
        <div><a>ТВ Сериал</a></div>
        <div>Эпизоды</div>
        <div>12 / 24</div>
        <div>Жанры</div>
        <div><a>Драма</a><a>Фэнтези</a></div>
        <div>Студия</div>
        <div><a>ZIG</a></div>
        <div>Статус</div>
        <div><a>Онгоинг</a></div>
        <div>Выпуск</div>
        <div>с 02.10.2026 по 25.12.2026</div>
    </div>
    <div class="description">Длинное описание сюжета аниме про свадьбу светлячков, которое точно длиннее сорока символов.</div>
</div>
"#;

// --- cards -------------------------------------------------------------------

#[test]
fn cards_parse_from_listing_grid() {
    let cards = parse_cards(&document(CARD_HTML));
    assert_eq!(cards.len(), 1, "one card: {cards:?}");
    let card = &cards[0];
    assert_eq!(card.id, "3815");
    assert_eq!(card.slug, "svad-ba-svetlyachkov");
    assert_eq!(card.title, "Свадьба светлячков");
    assert_eq!(card.alt_title.as_deref(), Some("Hotaru no Yomeiri"));
    assert_eq!(
        card.poster.as_deref(),
        Some("https://img.cdngos.com/v/250x350/anime/6a/6acd3a0f08d76086570449")
    );
    assert_eq!(card.score, Some(8.6));
}

#[test]
fn cards_parse_scoped_list_and_grid_metadata() {
    let list = r#"<div class="ani-list__item">
      <a class="d-block" href="/anime/example-123" title="Example"></a>
      <p class="ani-list__item-description">A synopsis.</p>
      <div class="ani-list__item-genres"><a href="/anime/genre/action">Action</a></div>
      <a href="/anime/season/2024">2024</a><a href="/anime/type/tv">TV</a>
    </div>"#;
    let card = &parse_cards(&document(list))[0];
    assert_eq!(card.description.as_deref(), Some("A synopsis."));
    assert_eq!(card.genres, vec!["Action"]);
    assert_eq!(card.year, Some(2024));
    assert_eq!(card.kind.as_deref(), Some("tv"));
    let preview = preview_from_card(card);
    assert_eq!(preview.genres, vec!["Action"]);
    assert_eq!(preview.description.as_deref(), Some("A synopsis."));
    assert_eq!(preview.year, Some(2024));
    assert_eq!(preview.kind.as_deref(), Some("tv"));
    assert_eq!(preview.source_ref, None);

    let grid = r#"<div class="ani-grid__item"><a href="/anime/example-124" title="Example"></a>
      <div class="ani-grid__item-genres"><span class="ani-grid__item-genres__link">Фильм</span>
      <span class="ani-grid__item-genres__link">2023</span></div></div>"#;
    let card = &parse_cards(&document(grid))[0];
    assert_eq!(card.kind.as_deref(), Some("movie"));
    assert_eq!(card.year, Some(2023));
    assert!(card.description.is_none());
    assert!(card.genres.is_empty());
    let preview = preview_from_card(card);
    assert_eq!(preview.kind.as_deref(), Some("movie"));
    assert_eq!(preview.year, Some(2023));
    assert!(preview.description.is_none());
    assert!(preview.genres.is_empty());
    assert_eq!(preview.source_ref, None);
}

#[test]
fn id_from_href_handles_plain_ids_and_rejects_gaps() {
    assert_eq!(id_from_href("/anime/naruto-70"), Some("70".to_string()));
    assert_eq!(id_from_href("/anime/top-250"), None);
    assert_eq!(id_from_href("/anime/random"), None);
}

#[test]
fn previews_stable_fields() {
    let cards = parse_cards(&document(CARD_HTML));
    let preview = preview_from_card(&cards[0]);
    assert_eq!(preview.id, "svad-ba-svetlyachkov-3815");
    assert_eq!(preview.title, "Свадьба светлячков");
    assert_eq!(preview.alternative_names.len(), 1);
    assert_eq!(preview.alternative_names[0].name, "Hotaru no Yomeiri");
}

// --- helpers ------------------------------------------------------------------

#[test]
fn status_and_kind_labels_map_to_canonical_values() {
    assert_eq!(airing_status_from_label("Онгоинг"), AiringStatus::Ongoing);
    assert_eq!(airing_status_from_label("Вышел"), AiringStatus::Released);
    assert_eq!(airing_status_from_label("Анонс"), AiringStatus::Anons);
    assert_eq!(airing_status_from_label("что-то"), AiringStatus::Unknown);
    assert_eq!(kind_from_label("ТВ Сериал"), Some("tv"));
    assert_eq!(kind_from_label("Фильм"), Some("movie"));
    assert_eq!(kind_from_label("OVA"), Some("ova"));
    assert_eq!(kind_from_label("Спешл"), Some("special"));
    assert_eq!(kind_from_label("Нет"), None);
}

#[test]
fn years_and_episode_counts_parse() {
    assert_eq!(year_from_text("с 02.10.2026 по 25.12.2026"), Some(2026));
    assert_eq!(year_from_text("нет года"), None);
    assert_eq!(episodes_count_from_label("12 / 24"), Some(12));
    assert_eq!(episodes_count_from_label("12"), Some(12));
    assert_eq!(episodes_count_from_label("---"), None);
}

#[test]
fn entities_decode() {
    assert_eq!(unescape_html("a&quot;b&amp;c&#39;d"), "a\"b&c'd");
}

// --- player buttons and episode pages -----------------------------------------

#[test]
fn voice_buttons_skip_broken_and_empty_labels() {
    let buttons = parse_voice_buttons(VOICE_BUTTONS_HTML);
    // " (ошибка)" and empty-label Sibnet are skipped.
    assert_eq!(buttons.len(), 3, "{buttons:?}");
    assert_eq!(buttons[0].label, "AniDUB");
    assert_eq!(buttons[0].player, "AniBoom");
    assert_eq!(
        buttons[0].embed,
        "https://aniboom.one/embed/AAAA1111?episode=1&translation=609"
    );
    assert_eq!(buttons[0].translation_id.as_deref(), Some("609"));
    assert_eq!(buttons[1].player, "CVH");
    assert_eq!(
        buttons[1].embed,
        "https://animego.me/cdn-iframe/51019/1/1?dubbing=AnilibriaTV"
    );
    assert_eq!(buttons[2].player, "Kodik");
}

#[test]
fn episode_pages_extract_number_and_page_id() {
    let pages = parse_episode_pages(&document(EPISODE_PAGES_HTML));
    assert_eq!(
        pages,
        vec![
            (1, "773273".to_string()),
            (2, "773274".to_string()),
            (3, "773275".to_string())
        ]
    );
    assert_eq!(
        episode_page_id(EPISODE_PAGES_HTML, 2).as_deref(),
        Some("773274")
    );
}

// --- schedule ------------------------------------------------------------------

#[test]
fn schedule_rows_parse_numbers_titles_and_release_state() {
    let rows = parse_schedule(SCHEDULE_HTML);
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(rows[0].number, 1);
    assert_eq!(rows[0].title.as_deref(), Some("Начало пути"));
    assert!(rows[0].released);
    assert_eq!(rows[1].number, 2);
    assert_eq!(rows[1].title, None, "'---' becomes no title");
    assert!(rows[1].released);
    assert_eq!(rows[2].number, 3);
    assert!(!rows[2].released);
}

// --- release -------------------------------------------------------------------

#[test]
fn release_extracts_tracker_ids_from_reference_links() {
    let refs = r#"<div class="content-ref-ids">
      <a href="//shikimori.io/animes/63431"><span>7.04</span></a>
      <a href="http://world-art.ru/animation/animation.php?id=12789"></a>
      <a href="//kinopoisk.ru/film/14669309"></a>
      <a href="//myanimelist.net/anime/63431"><span>7.36</span></a>
    </div>"#;
    let (release, _) = map_release(&document(&format!("{ENTITY_HTML}{refs}")), "3815").unwrap();
    let values: Vec<_> = release.tracker_data.iter().map(|item| (item.name.as_str(), item.data.as_str())).collect();
    assert_eq!(values, vec![("Shikimori", "63431"), ("WorldArt", "12789"), ("Kinopoisk", "14669309"), ("MyAnimeList", "63431")]);
}

#[test]
fn tracker_links_accept_aliases_slugs_and_remove_duplicates() {
    let document = dom_query::Document::from(r#"<div class="content-ref-ids">
      <a href="https://www.shikimori.one/animes/00020-naruto?x=1#details"></a>
      <a href="https://shikimori.me/animes/20"></a>
      <a href="https://myanimelist.net/anime/20/Naruto"></a>
      <a href="https://myanimelist.net/anime/20/"></a>
      <a href="https://www.kinopoisk.ru/series/123/"></a>
    </div>"#);
    let values = release_tracker_data(&document);
    assert_eq!(values.len(), 3);
    assert_eq!(values[0].name, "Shikimori");
    assert_eq!(values[0].data, "20");
    assert_eq!(values[1].data, "20");
    assert_eq!(values[2].data, "123");
}

#[test]
fn tracker_links_ignore_invalid_ids_domains_and_unrelated_links() {
    let references = dom_query::Document::from(r#"
      <a href="https://myanimelist.net/anime/99">Outside reference block</a>
      <div class="content-ref-ids">
        <a href="https://myanimelist.net.evil.test/anime/20"></a>
        <a href="https://evil.test/?url=https://myanimelist.net/anime/20"></a>
        <a href="https://myanimelist.net/manga/20"></a>
        <a href="https://shikimori.io/animes/0"></a>
        <a href="https://myanimelist.net/anime/-1"></a>
        <a href="https://myanimelist.net/anime/12oops"></a>
        <a href="https://myanimelist.net/anime/9999999999999999999999999999999999"></a>
        <a href="https://world-art.ru/animation/animation.php?id=nope"></a>
        <a href="https://kinopoisk.ru/name/123"></a>
        <a href="javascript:alert(1)"></a><a href="/anime/20"></a><a></a>
      </div>"#);
    assert!(release_tracker_data(&references).is_empty());
    let (release, _) = map_release(&document(ENTITY_HTML), "3815").unwrap();
    assert!(release.tracker_data.is_empty());
}

#[test]
fn release_page_maps_fields_and_other_info() {
    let (release, _) = map_release(&document(ENTITY_HTML), "3815").expect("release maps");
    assert_eq!(release.id, "3815");
    assert_eq!(release.name, "Свадьба светлячков");
    assert!(release
        .alternative_names
        .iter()
        .any(|name| name.name == "Hotaru no Yomeiri"));
    assert!(release
        .alternative_names
        .iter()
        .any(|name| name.name == "Кладбище светлячков"));
    assert_eq!(release.kind.as_deref(), Some("tv"));
    assert_eq!(release.status, AiringStatus::Ongoing);
    assert_eq!(release.year, Some(2026));
    assert_eq!(release.total_episodes, Some(12));
    assert_eq!(release.rating, Some(8.6));
    assert_eq!(release.genres.iter().map(|genre| genre.name.as_str()).collect::<Vec<_>>(), vec!["Драма", "Фэнтези"]);
    assert_eq!(release.studios, vec!["ZIG"]);
    assert!(release
        .other_info
        .iter()
        .any(|block| block.title == "Жанры" && block.data.len() >= 2));
    assert!(release
        .description
        .as_deref()
        .is_some_and(|text| text.contains("свадьбу светлячков")));
    assert!(release
        .source_ref
        .as_deref()
        .is_some_and(|text| text.contains("3815")));
}

#[test]
fn release_missing_entity_is_an_error() {
    assert!(map_release(&document("<div>nothing</div>"), "1").is_err());
}

// --- candidates -----------------------------------------------------------------

#[test]
fn player_candidates_rank_players_and_carry_payload() {
    let candidates = player_candidates_for(TEST_BASE, VOICE_BUTTONS_HTML, 1);
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0].protocol, "cvh.embed.v1");
    assert_eq!(candidates[1].protocol, "kodik.embed.v1");
    assert_eq!(candidates[2].protocol, "aniboom.embed.v1");
    let payload: EmbedPayload = serde_json::from_str(&candidates[2].payload).unwrap();
    assert_eq!(
        payload.variants[0].url,
        "https://aniboom.one/embed/AAAA1111?episode=1&translation=609"
    );
    assert_eq!(payload.variants[0].label, "AniDUB");
    assert_eq!(payload.referer.as_deref(), Some("https://animego.me/"));
}

#[test]
fn xhr_content_accepts_both_envelopes() {
    let flat: XhrContent = serde_json::from_str(r#"{"content":"<b>1</b>"}"#).unwrap();
    assert_eq!(flat.content(), Some("<b>1</b>"));
    let nested: XhrContent = serde_json::from_str(r#"{"data":{"content":"<b>2</b>"}}"#).unwrap();
    assert_eq!(nested.content(), Some("<b>2</b>"));
    let empty: XhrContent = serde_json::from_str(r#"{"data":{"content":""}}"#).unwrap();
    assert_eq!(empty.content(), None);
}

// --- urls -----------------------------------------------------------------------

const TEST_BASE: &str = "https://animego.me";

#[test]
fn list_urls_carry_page_sort_and_type() {
    let url = build_list_url_for(TEST_BASE, 2, "anime", None, &[], "POPULARITY");
    assert!(url.contains("/anime/2?"), "{url}");
    assert!(url.contains("sort=rating"), "{url}");
    assert!(url.contains("view=list"), "{url}");

    let url = build_list_url_for(
        TEST_BASE,
        1,
        "anime",
        None,
        &[("type".to_string(), "movie".to_string())],
        "RATE",
    );
    assert!(url.contains("sort=rating"), "{url}");
    assert!(url.contains("/anime/type/movie?"), "{url}");

    let url = build_list_url_for(TEST_BASE, 1, "search/anime", Some("slime"), &[], "RATE");
    assert!(url.contains("search/anime?q=slime"), "{url}");
    assert!(
        !url.contains("page="),
        "first search page omits page: {url}"
    );
    assert!(!url.contains("view=list"));
}

#[test]
fn release_urls_are_absolute() {
    let url = release_url_for(TEST_BASE, "izdaleka-3839").unwrap();
    assert!(
        url.starts_with("https://animego.me/anime/izdaleka-3839"),
        "{url}"
    );
}

#[test]
fn source_ref_carries_animego_id() {
    let json: serde_json::Value =
        serde_json::from_str(&source_ref("3839", "izdaleka").unwrap()).unwrap();
    assert_eq!(json["animegoId"], "3839");
}

#[test]
fn canonical_release_ids_survive_navigation_without_opaque_payload() {
    let preview = preview_from_card(&parse_cards(&document(CARD_HTML))[0]);
    assert_eq!(numeric_release_id(&preview.id).unwrap(), "3815");
    assert_eq!(
        release_url_for(TEST_BASE, &preview.id).unwrap(),
        "https://animego.me/anime/svad-ba-svetlyachkov-3815"
    );
    assert!(numeric_release_id("").is_err());
}
#[test]
fn provider_candidates_group_dubs_and_keep_cvh_integration_in_the_source() {
    let html = r#"<button data-player="//animego.me/cdn-iframe/52991/Dream%20Cast/2/3" data-provider-title="CVH" data-translation-title="Dream Cast" data-ptranslation="1"></button>
    <button data-player="//animego.me/cdn-iframe/52991/AnilibriaTV/2/3" data-provider-title="CVH" data-translation-title="AniLibria" data-ptranslation="2"></button>"#;
    let candidates = player_candidates_for(TEST_BASE, html, 3);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].label.as_deref(), Some("CVH"));
    let payload: EmbedPayload = serde_json::from_str(&candidates[0].payload).unwrap();
    assert_eq!(payload.variants.len(), 2);
    let context = payload.variants[0].cvh.as_ref().unwrap();
    assert_eq!(context.voice, "Dream Cast");
    assert_eq!(context.season, 2);
    assert_eq!(context.episode, 3);
    assert_eq!(context.publisher_id, "747");
}

#[test]
fn list_layout_and_infinite_scroll_are_supported() {
    let html = CARD_HTML.replace("ani-grid__item", "ani-list__item");
    assert_eq!(parse_cards(&document(&html)).len(), 1);
    assert!(has_next_page(
        &document(r#"<div data-infinite-scroll-has-more-value="true"></div>"#),
        1
    ));
    assert!(!has_next_page(
        &document(r#"<div data-infinite-scroll-has-more-value="false"></div>"#),
        1
    ));
}

#[test]
fn listing_page_keeps_card_identity_and_pagination() {
    let html = format!(
        r#"{CARD_HTML}{CARD_HTML}<ul class="pagination"><li><a href="/anime?page=2">2</a></li></ul>"#
    );
    let document = document(&html);
    let page = listing_page(&document, 1);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, "svad-ba-svetlyachkov-3815");
    assert_eq!(page.meta.per_page, 1);
    assert!(page.meta.has_next);
    assert!(!listing_page(&document, u32::MAX).meta.has_next);
}

#[test]
fn ui_sort_selections_produce_distinct_site_requests() {
    let mut urls = std::collections::HashSet::new();
    for (value, expected) in [
        ("RATE", "rating"),
        ("UPDATED", "createdAt"),
        ("NEW", "startDate"),
    ] {
        for encoded in [
            value.to_string(),
            serde_json::json!(value).to_string(),
            serde_json::json!({"value":value,"direction":""}).to_string(),
        ] {
            let params = CatalogParams {
                page: 1,
                search: None,
                sort: Some(encoded),
                filters: vec![],
            };
            let url = build_list_url_for(TEST_BASE, 1, "anime", None, &[], &sort_of(&params));
            assert!(url.contains(&format!("sort={expected}")), "{url}");
            urls.insert(url);
        }
    }
    assert_eq!(urls.len(), 3);
    let url = build_list_url_for(
        TEST_BASE,
        1,
        "anime",
        None,
        &[("type".into(), "\"movie\"".into())],
        "RATE",
    );
    assert!(url.contains("/anime/type/movie?"), "{url}");
    assert!(!url.contains("%22"));
}

#[test]
fn fullmetal_alchemist_live_markup_preserves_release_metadata() {
    let html = include_str!("fixtures/fullmetal-alchemist.html");
    let (release, _) = map_release(&document(html), "stalnoy-alhimik-bratstvo-126").unwrap();
    assert_eq!(release.name, "Стальной алхимик: Братство");
    assert_eq!(release.kind.as_deref(), Some("tv"));
    assert_eq!(release.status, AiringStatus::Released);
    assert_eq!(release.year, Some(2009));
    assert_eq!(release.total_episodes, Some(64));
    assert_eq!(release.studios, vec!["Bones"]);
    assert_eq!(
        release.alternative_names[0].name,
        "Fullmetal Alchemist: Brotherhood"
    );
    assert_eq!(release.rating, Some(9.3));
    let payload: serde_json::Value =
        serde_json::from_str(release.source_ref.as_deref().unwrap()).unwrap();
    assert_eq!(payload["episodeDurationSeconds"], 1440);
    assert_eq!(
        release.genres.iter().map(|genre| genre.name.as_str()).collect::<Vec<_>>(),
        vec!["Экшен", "Приключения", "Драма", "Фэнтези", "Сёнен"]
    );
    assert!(release.description.unwrap().contains("Эдвард"));
    assert!(release.poster.unwrap().contains("6763fca60d079276981979"));
    assert!(!strip_heavy_nodes(html).contains("ignored()"));
}

#[test]
fn cvh_query_voice_urls_keep_native_context() {
    let context = cvh_context("https://animego.me/cdn-iframe/37105/1/1?dubbing=AniDUB", 1).unwrap();
    assert_eq!(context.media_id, "37105");
    assert_eq!(context.voice, "AniDUB");
    assert_eq!(context.season, 1);
    assert_eq!(context.episode, 1);
    let context = cvh_context(
        "https://animego.me/cdn-iframe/37105/2/3?dubbing=Dream%20Cast",
        3,
    )
    .unwrap();
    assert_eq!(context.voice, "Dream Cast");
    assert_eq!(context.season, 2);
    assert_eq!(context.episode, 3);
    assert!(cvh_context("https://animego.me/cdn-iframe/37105/1/1", 1).is_none());
}
