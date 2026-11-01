//! Audience statistics from the MAL `/stats` page.
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/Parser/Anime/AnimeStatsParser.php` — Watching/Completed/On-Hold/
//!   Dropped/Plan to Watch, Total and the `Score Stats` table.
//! - `src/Parser/Manga/MangaStatsParser.php` — Reading/Plan to Read labels.
//! - `src/Request/Anime/AnimeStatsRequest.php`, `MangaStatsRequest.php` —
//!   `/anime/<id>/_/stats` and `/manga/<id>/_/stats`.
//! - `src/Model/Anime/AnimeStatsScore.php` — score, votes, percentage rows.
//!
//! The score table prints percentages before the vote count
//! (`18.8% (409098 votes)`); vote counts are read from `<small>` first and from
//! the parenthesised group only as a fallback, so a percentage can never be
//! mistaken for a vote count.

use crate::animex::module::types::{ScoreCount, StatusCount, TitleStatistics};
use dom_query::Document;
use std::collections::BTreeMap;

use crate::html::{clean_text, label_count, label_text, select_text};
use crate::model::{count_from_text, score_from_text};

pub fn statistics(document: &Document, manga: bool) -> TitleStatistics {
    let rating = select_text(document, "span[itemprop='ratingValue']")
        .and_then(|text| text.split_whitespace().next().and_then(score_from_text))
        .or_else(|| {
            label_text(document, &["Score"])
                .and_then(|text| text.split_whitespace().next().and_then(score_from_text))
        });

    let completed_label = if manga {
        "Прочитано"
    } else {
        "Просмотрено"
    };
    let watching_label = if manga { "Читаю" } else { "Смотрю" };
    let statuses = [
        (
            if manga { "Reading" } else { "Watching" },
            "watching",
            watching_label,
        ),
        ("Completed", "completed", completed_label),
        ("On-Hold", "on_hold", "Отложено"),
        ("Dropped", "dropped", "Брошено"),
        (
            if manga {
                "Plan to Read"
            } else {
                "Plan to Watch"
            },
            "planned",
            "Запланировано",
        ),
    ]
    .into_iter()
    .filter_map(|(source, status, label)| {
        label_count(document, &[source]).map(|count| StatusCount {
            status: status.to_string(),
            label: label.to_string(),
            count,
        })
    })
    .collect();

    let mut buckets = BTreeMap::<u8, u32>::new();
    let score_rows = {
        let adjacent = document.select("h2#score_stats + table tr");
        if adjacent.exists() {
            adjacent
        } else {
            document.select("h2#score_stats ~ table tr")
        }
    };
    for row in score_rows.iter() {
        let selection = dom_query::Selection::from(row);
        let score = selection
            .select("td:first-child")
            .text()
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|score| (1..=10).contains(score));
        let Some(score) = score else {
            continue;
        };
        let small = clean_text(&selection.select("td:nth-child(2) small").text());
        let votes = count_from_text(&small).or_else(|| {
            let cell = clean_text(&selection.select("td:nth-child(2)").text());
            count_from_text(cell.split('(').nth(1).unwrap_or_default())
        });
        if let Some(votes) = votes.filter(|votes| *votes > 0) {
            let bucket = buckets.entry(score).or_default();
            *bucket = bucket.saturating_add(votes);
        }
    }

    TitleStatistics {
        rating,
        score_scale: 10,
        scores: buckets
            .into_iter()
            .map(|(score, count)| ScoreCount { score, count })
            .collect(),
        statuses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::document;

    #[test]
    fn anime_statuses_and_score_votes_are_read_separately() {
        let statistics = statistics(
            &document(include_str!("../../tests/fixtures/statistics.html")),
            false,
        );
        assert_eq!(statistics.rating, Some(8.03));
        assert_eq!(statistics.score_scale, 10);
        assert_eq!(statistics.statuses.len(), 5);
        assert_eq!(
            statistics.statuses.iter().map(|row| row.count).sum::<u32>(),
            226_830 + 2_589_982 + 99_852 + 141_272 + 115_884
        );
        let scores: u32 = statistics.scores.iter().map(|row| row.count).sum();
        assert_eq!(scores, 409_098 + 420_125 + 603_961);
        assert!(statistics
            .scores
            .iter()
            .all(|row| (1..=10).contains(&row.score)));
        assert!(statistics
            .statuses
            .iter()
            .all(|row| row.status != "favorites" && row.status != "total"));
    }

    #[test]
    fn manga_uses_reading_and_plan_to_read_labels() {
        let statistics = statistics(
            &document(
                r#"<html><body>
                <span itemprop="ratingValue">9.46</span>
                <div class="spaceit_pad"><span class="dark_text">Reading:</span> 440,596</div>
                <div class="spaceit_pad"><span class="dark_text">Completed:</span> 108,282</div>
                <div class="spaceit_pad"><span class="dark_text">On-Hold:</span> 69,149</div>
                <div class="spaceit_pad"><span class="dark_text">Dropped:</span> 14,771</div>
                <div class="spaceit_pad"><span class="dark_text">Plan to Read:</span> 180,289</div>
                <h2 id="score_stats">Score Stats</h2>
                <table><tr><td>10</td><td><div><span>50.0% <small>(100 votes)</small></span></div></td></tr></table>
                </body></html>"#,
            ),
            true,
        );
        assert_eq!(statistics.rating, Some(9.46));
        assert_eq!(statistics.statuses[0].label, "Читаю");
        assert_eq!(statistics.statuses[0].status, "watching");
        assert_eq!(statistics.statuses[1].label, "Прочитано");
        assert_eq!(statistics.statuses[4].status, "planned");
        assert_eq!(statistics.scores[0].count, 100);
    }

    #[test]
    fn empty_statistics_page_has_no_fabricated_counts() {
        let statistics = statistics(&document("<html><body></body></html>"), false);
        assert!(statistics.rating.is_none());
        assert!(statistics.scores.is_empty());
        assert!(statistics.statuses.is_empty());
    }
}
