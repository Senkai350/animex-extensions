//! Parsers grouped by MAL page type.
//!
//! Each module documents the original jikan-me/jikan parser it is modelled on:
//! - `anime`      ← `src/Parser/Anime/AnimeParser.php`
//! - `search`     ← `src/Parser/Search/AnimeSearchParser.php`
//! - `characters` ← `src/Parser/Anime/CharactersAndStaffParser.php`
//! - `videos`     ← `src/Parser/Anime/VideosParser.php`
//! - `pictures`   ← `src/Parser/Common/PicturesPageParser.php`
//! - `statistics` ← `src/Parser/Anime/AnimeStatsParser.php`
//! - `recommendations` ← `src/Parser/Common/Recommendations.php`

pub mod anime;
pub mod characters;
pub mod episodes;
pub mod pictures;
pub mod recommendations;
pub mod search;
pub mod statistics;
pub mod videos;
