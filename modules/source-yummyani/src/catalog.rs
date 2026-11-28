mod genres;

use std::ops::RangeInclusive;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::animex::module::types::{
    CatalogParams, FilterField, FilterMultiselect, FilterRange, OptionPair,
};
use crate::exports::animex::module::catalog_sorting_api::SortingConfig;
use crate::{api_url_for, rfc3339_utc, PAGE_SIZE, PROBE_SIZE};

use genres::GENRES;

const TYPES: &[(&str, &str)] = &[
    ("1", "Сериал"),
    ("2", "Полнометражный фильм"),
    ("3", "Короткометражный фильм"),
    ("4", "OVA"),
    ("5", "Спешл"),
    ("6", "Малометражный сериал"),
    ("7", "ONA"),
];
const STATUSES: &[(&str, &str)] = &[
    ("released", "Вышел"),
    ("ongoing", "Онгоинг"),
    ("announcement", "Анонс"),
];
const SEASONS: &[(&str, &str)] = &[("1", "Зима"), ("2", "Весна"), ("3", "Лето"), ("4", "Осень")];
const AGES: &[(&str, &str)] = &[
    ("1", "G (для всех возрастов)"),
    ("2", "PG (для детей)"),
    ("3", "PG-13 (от 13 лет)"),
    ("4", "R-17+"),
    ("5", "R+"),
];
const SORTS: &[(&str, &str)] = &[
    ("top", "По релевантности"),
    ("title", "По названию"),
    ("year", "По году выпуска"),
    ("rating", "По рейтингу"),
    ("rating_counters", "По числу голосов"),
    ("views", "По просмотрам"),
    ("random", "Случайное аниме по фильтру"),
];
const DIRECTIONS: &[(&str, &str)] = &[
    ("auto", "Порядок источника"),
    ("asc", "По возрастанию"),
    ("desc", "По убыванию"),
];

const MIN_YEAR: u32 = 1900;
const MAX_EPISODES: u32 = 999_999;

pub(super) fn filters() -> Vec<FilterField> {
    vec![
        multiselect("genres", "Жанры", GENRES),
        multiselect("exclude_genres", "Исключить жанры", GENRES),
        multiselect("types", "Тип аниме", TYPES),
        multiselect("status", "Статус аниме", STATUSES),
        range("year", "Год", MIN_YEAR, max_year()),
        multiselect("season", "Сезон", SEASONS),
        range("episodes", "Количество серий", 0, MAX_EPISODES),
        multiselect("min_age", "Возрастной рейтинг", AGES),
    ]
}

pub(super) fn sorting() -> SortingConfig {
    SortingConfig {
        id: "sort".to_string(),
        label: "Сортировка".to_string(),
        options: labeled_options(SORTS),
        default: Some("top".to_string()),
        directions: labeled_options(DIRECTIONS),
        default_direction: Some("auto".to_string()),
    }
}

pub(super) fn url(
    base: &str,
    query: Option<&str>,
    params: &CatalogParams,
) -> Result<String, String> {
    let sort = sort_of(params)?;
    let random = sort.is_some_and(|(value, _)| value == "random");
    let (limit, offset) = if random {
        (1, 0)
    } else {
        (
            PROBE_SIZE,
            params.page.saturating_sub(1).saturating_mul(PAGE_SIZE),
        )
    };
    let mut args = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
    if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) {
        args.push(("q", query.to_string()));
    }
    if let Some((value, forward)) = sort {
        args.push(("sort", value.to_string()));
        if let Some(forward) = forward {
            args.push(("sort_forward", forward.to_string()));
        }
    }
    for (id, options) in [
        ("genres", GENRES),
        ("exclude_genres", GENRES),
        ("types", TYPES),
        ("status", STATUSES),
        ("season", SEASONS),
        ("min_age", AGES),
    ] {
        if let Some(value) = filter_value(params, id) {
            if let Some(selection) = choices(value, id, options)? {
                args.push((id, selection));
            }
        }
    }
    for (id, keys, allowed) in [
        ("year", ["from_year", "to_year"], MIN_YEAR..=max_year()),
        ("episodes", ["ep_from", "ep_to"], 0..=MAX_EPISODES),
    ] {
        if let Some(value) = filter_value(params, id) {
            for (key, bound) in keys.into_iter().zip(bounds(value, id, allowed)?) {
                if let Some(bound) = bound {
                    args.push((key, bound.to_string()));
                }
            }
        }
    }
    api_url_for(base, "/api/anime", &args)
}

fn max_year() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| rfc3339_utc(time.as_secs()))
        .and_then(|date| date[..4].parse::<u32>().ok())
        .unwrap_or(2026)
        + 10
}

fn labeled_options(values: &[(&str, &str)]) -> Vec<OptionPair> {
    values
        .iter()
        .map(|(value, label)| OptionPair {
            value: (*value).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}

fn multiselect(id: &str, label: &str, values: &[(&str, &str)]) -> FilterField {
    FilterField::Multiselect(FilterMultiselect {
        id: id.to_string(),
        label: label.to_string(),
        options: labeled_options(values),
        default: Vec::new(),
    })
}

fn range(id: &str, label: &str, min: u32, max: u32) -> FilterField {
    FilterField::Range(FilterRange {
        id: id.to_string(),
        label: label.to_string(),
        min: min as f32,
        max: max as f32,
        step: Some(1.0),
        default: None,
    })
}

fn filter_value<'a>(params: &'a CatalogParams, id: &str) -> Option<&'a str> {
    params
        .filters
        .iter()
        .find(|(key, _)| key == id)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty() && *value != "null")
}

fn choices(value: &str, id: &str, options: &[(&str, &str)]) -> Result<Option<String>, String> {
    let selected = if value.starts_with('[') {
        serde_json::from_str::<Vec<String>>(value)
            .map_err(|_| format!("yummyani: invalid {id} filter"))?
    } else {
        // Older versions sent a single status as a string.
        vec![serde_json::from_str::<String>(value).unwrap_or_else(|_| value.to_string())]
    };
    if selected
        .iter()
        .any(|value| !options.iter().any(|(key, _)| key == value))
    {
        return Err(format!("yummyani: unsupported {id} filter"));
    }
    Ok((!selected.is_empty()).then(|| selected.join(",")))
}

fn bounds(value: &str, id: &str, allowed: RangeInclusive<u32>) -> Result<[Option<u32>; 2], String> {
    let bounds: [Option<u32>; 2] =
        serde_json::from_str(value).map_err(|_| format!("yummyani: invalid {id} range"))?;
    if bounds
        .iter()
        .flatten()
        .any(|bound| !allowed.contains(bound))
    {
        return Err(format!("yummyani: unsupported {id} range"));
    }
    Ok(bounds)
}

fn sort_of(params: &CatalogParams) -> Result<Option<(&'static str, Option<bool>)>, String> {
    let Some(raw) = params
        .sort
        .as_deref()
        .or_else(|| filter_value(params, "sort"))
    else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<Value>(raw).ok();
    let (value, direction) = match parsed.as_ref() {
        Some(Value::Object(object)) => (
            object
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            object
                .get("direction")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        Some(Value::String(value)) => (value.as_str(), ""),
        _ => (raw.trim(), ""),
    };
    if value.is_empty() {
        return Ok(None);
    }
    let value = SORTS
        .iter()
        .map(|(key, _)| *key)
        .chain(["id"]) // Accepted by prior versions of the extension.
        .find(|key| *key == value)
        .ok_or_else(|| "yummyani: unsupported sort value".to_string())?;
    let forward = match direction {
        "" => None,
        // The API's forward order for relevance puts the best matches first.
        "auto" => Some(matches!(value, "top" | "title")),
        "asc" => Some(true),
        "desc" => Some(false),
        _ => return Err("yummyani: unsupported sort direction".to_string()),
    };
    Ok(Some((
        value,
        if value == "random" { None } else { forward },
    )))
}
