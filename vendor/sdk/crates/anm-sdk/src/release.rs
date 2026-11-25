/// Define conversion helpers for the release types generated in a guest world.
#[macro_export]
macro_rules! release_metadata {
    () => {
        mod release_metadata {
            use crate::animex::module::types as w;

            pub fn upgrade(value: w::LegacyRelease) -> w::Release {
                w::Release {
                    id: value.id,
                    name: value.name,
                    alternative_names: value.alternative_names,
                    tracker_data: value.tracker_data,
                    other_info: value.other_info,
                    banner_url: value.banner_url,
                    poster: value.poster,
                    description: value.description,
                    genres: value.genres.into_iter().map(|name| w::Genre {
                        name, id: None, language: None, url: None,
                    }).collect(),
                    studios: value.studios,
                    status: value.status,
                    kind: value.kind,
                    year: value.year,
                    trailer: value.trailer,
                    total_episodes: value.total_episodes,
                    next_episode_at: value.next_episode_at,
                    rating: value.rating,
                    source_ref: value.source_ref,
                    episode_duration_seconds: None,
                    aired_from: None,
                    aired_to: None,
                    regional_premieres: Vec::new(),
                    age_rating: None,
                    original_source: None,
                    score_scale: None,
                    url: None,
                }
            }

            #[allow(dead_code)]
            pub fn date(year: i32, month: Option<i32>, day: Option<i32>) -> Option<w::CalendarDate> {
                let year = u16::try_from(year).ok().filter(|year| (1..=9999).contains(year))?;
                let month = month.filter(|month| (1..=12).contains(month)).map(|month| month as u8);
                let day = day.filter(|day| (1..=31).contains(day) && month.is_some()).map(|day| day as u8);
                Some(w::CalendarDate { year, month, day })
            }

            #[allow(dead_code)]
            pub fn iso_date(value: &str) -> Option<w::CalendarDate> {
                let mut parts = value.split('T').next()?.split('-');
                let year = parts.next()?.parse().ok()?;
                let month = parts.next().and_then(|part| part.parse().ok());
                let day = parts.next().and_then(|part| part.parse().ok());
                date(year, month, day)
            }

            #[allow(dead_code)]
            pub fn duration_text(value: &str) -> Option<u32> {
                let mut parts = value.split_whitespace().peekable();
                let mut seconds = 0_u32;
                while let Some(part) = parts.next() {
                    let Ok(number) = part.parse::<u32>() else { continue };
                    let factor = match parts.peek().map(|unit| unit.trim_end_matches('.')) {
                        Some("hr" | "h" | "ч") => 3600,
                        Some("min" | "m" | "мин") => 60,
                        Some("sec" | "s" | "сек") => 1,
                        _ => continue,
                    };
                    parts.next();
                    seconds = seconds.checked_add(number.checked_mul(factor)?)?;
                }
                (seconds > 0).then_some(seconds)
            }

            #[allow(dead_code)]
            pub fn age_rating(value: &str) -> Option<w::AgeRating> {
                Some(match value.to_ascii_lowercase().replace(['_', ' '], "-").as_str() {
                    "none" | "not-rated" => w::AgeRating::None,
                    "g" => w::AgeRating::G,
                    "pg" => w::AgeRating::Pg,
                    "pg-13" => w::AgeRating::Pg13,
                    "r" | "r-17" | "r-17+" => w::AgeRating::R,
                    "r+" | "r-plus" => w::AgeRating::RPlus,
                    "rx" => w::AgeRating::Rx,
                    "0+" | "r0-plus" | "ru-0-plus" => w::AgeRating::Ru0Plus,
                    "6+" | "r6-plus" | "ru-6-plus" => w::AgeRating::Ru6Plus,
                    "12+" | "r12-plus" | "ru-12-plus" => w::AgeRating::Ru12Plus,
                    "16+" | "r16-plus" | "ru-16-plus" => w::AgeRating::Ru16Plus,
                    "18+" | "r18-plus" | "ru-18-plus" => w::AgeRating::Ru18Plus,
                    _ => return None,
                })
            }

            #[allow(dead_code)]
            pub fn original_source(value: &str) -> Option<w::OriginalSource> {
                Some(match value.trim().to_ascii_lowercase().replace(['_', ' '], "-").as_str() {
                    "original" | "оригинал" => w::OriginalSource::Original,
                    "manga" | "манга" => w::OriginalSource::Manga,
                    "web-manga" | "веб-манга" | "digital-manga" | "manhwa" | "manhua" => w::OriginalSource::WebManga,
                    "four-koma-manga" | "4-koma-manga" => w::OriginalSource::FourKomaManga,
                    "novel" | "роман" => w::OriginalSource::Novel,
                    "web-novel" | "веб-роман" => w::OriginalSource::WebNovel,
                    "visual-novel" | "визуальная-новелла" => w::OriginalSource::VisualNovel,
                    "light-novel" | "ранобэ" => w::OriginalSource::LightNovel,
                    "game" | "video-game" | "игра" => w::OriginalSource::Game,
                    "card-game" => w::OriginalSource::CardGame,
                    "music" | "музыка" => w::OriginalSource::Music,
                    "radio" => w::OriginalSource::Radio,
                    "book" | "книга" => w::OriginalSource::Book,
                    "picture-book" => w::OriginalSource::PictureBook,
                    "mixed-media" => w::OriginalSource::MixedMedia,
                    "other" | "другое" => w::OriginalSource::Other,
                    "unknown" | "неизвестно" => w::OriginalSource::Unknown,
                    _ => return None,
                })
            }
        }
    };
}
