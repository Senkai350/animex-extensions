//! Characters and staff for anime and manga.
//!
//! References from the original repository (jikan-me/jikan):
//! - `src/Parser/Anime/CharactersAndStaffParser.php` — characters come from
//!   `div.anime-character-container`, staff from the tables after the Staff
//!   heading.
//! - `src/Parser/Anime/StaffListItemParser.php` — person link, image and
//!   `<small>` positions (roles).
//! - `src/Parser/Character/CharacterListItemParser.php` — `h3_character_name`,
//!   favorite count, role and all voice-actor rows.
//! - `src/Parser/Character/VoiceActorParser.php` — person/language per VA row.
//! - `src/Parser/Manga/CharactersParser.php` — manga uses
//!   `div.manga-character-container` and has no voice actors.
//!
//! `Character.seiyuu` has no language field in the AniMex ABI. Jikan exposes
//! every language; here we prefer Japanese rows (the original cast) and fall
//! back to all rows only when MAL has no Japanese entry. This avoids showing
//! English dub actors as if they were the Japanese seiyuu.

use crate::animex::module::types::{Character, StaffMember, StaffRef};
use dom_query::{Document, NodeRef, Selection};
use std::collections::HashSet;

use crate::html::{clean_text, image_url, node_text};
use crate::model::{id_from_href, tracker_links};

pub fn characters(document: &Document) -> Vec<Character> {
    let mut tables = document
        .select("table.js-anime-character-table, table.js-manga-character-table")
        .nodes()
        .to_vec();
    if tables.is_empty() {
        tables = document
            .select("div.anime-character-container table, div.manga-character-container table")
            .nodes()
            .to_vec();
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for table in tables {
        let Some(character) = parse_list_character(&table) else {
            continue;
        };
        if seen.insert(character.id.clone()) {
            result.push(character);
        }
    }
    result
}

/// Fallback for the top-8/10 characters embedded in the detail page.
pub fn characters_from_detail(document: &Document) -> Vec<Character> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for table in document
        .select("div.detail-characters-list table")
        .nodes()
        .iter()
        .cloned()
    {
        let Some(character) = parse_detail_character(&table) else {
            continue;
        };
        if seen.insert(character.id.clone()) {
            result.push(character);
        }
    }
    result
}

/// Role metadata comes from the already fetched cast page; no character requests.
pub fn characters_with_roles(document: &Document) -> Vec<crate::exports::animex::module::tracker_character_roles_api::CharacterWithRole> {
    let mut cast = characters(document);
    if cast.is_empty() { cast = characters_from_detail(document); }
    let mut roles = std::collections::HashMap::new();
    for table in document.select("table").nodes() {
        let selection = Selection::from(table.clone());
        if selection.select("h3.h3_character_name, h3.h3_characters_voice_actors").nodes().len() != 1 { continue; }
        let Some(character) = parse_list_character(table).or_else(|| parse_detail_character(table)) else { continue; };
        let role = selection.select(".spaceit_pad, small").nodes().iter().find_map(|node| {
            let text = node_text(node)?.to_ascii_lowercase();
            match text.trim() { "main" => Some("main".to_string()), "supporting" => Some("supporting".to_string()), _ => None }
        });
        if let Some(role) = role { roles.insert(character.id, role); }
    }
    cast.into_iter().map(|mut character| {
        character.seiyuu.clear();
        crate::exports::animex::module::tracker_character_roles_api::CharacterWithRole { role: roles.get(&character.id).cloned(), character }
    }).collect()
}

pub fn staff(document: &Document) -> Vec<StaffMember> {
    let mut tables = staff_tables(document);
    if tables.is_empty() {
        tables = staff_tables_from_heading(document);
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for table in tables {
        let selection = Selection::from(table);
        let Some(link) = person_link(&selection) else {
            continue;
        };
        let Some(id) = link
            .attr("href")
            .and_then(|href| id_from_href(&href.to_string(), "people"))
        else {
            continue;
        };
        let Some(name) = node_text(&link) else {
            continue;
        };
        let role = clean_text(&selection.select("small").text());
        let role = if role.is_empty() {
            clean_text(&selection.select(".spaceit_pad").text())
        } else {
            role
        };
        if !seen.insert((id, role.clone())) {
            continue;
        }
        result.push(StaffMember {
            id: id.to_string(),
            name,
            alternative_names: Vec::new(),
            role,
            image: selection.select("img").nodes().first().and_then(image_url),
        });
    }
    result
}

/// Fallback for the Staff block on the title detail page.
pub fn staff_from_detail(document: &Document) -> Vec<StaffMember> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let Some(heading) = document
        .select("h2")
        .nodes()
        .iter()
        .cloned()
        .find(|node| clean_text(&node.text()).eq_ignore_ascii_case("Staff"))
    else {
        return result;
    };
    let mut tables = Vec::new();
    if let Some(next) = heading.next_element_sibling() {
        tables.extend(tables_until_next_heading(next));
    }
    if let Some(parent_next) = heading
        .parent()
        .and_then(|parent| parent.next_element_sibling())
    {
        tables.extend(tables_until_next_heading(parent_next));
    }
    for table in tables {
        let selection = Selection::from(table);
        let Some(link) = person_link(&selection) else {
            continue;
        };
        let Some(id) = link
            .attr("href")
            .and_then(|href| id_from_href(&href.to_string(), "people"))
        else {
            continue;
        };
        let Some(name) = node_text(&link) else {
            continue;
        };
        let role = clean_text(&selection.select("small").text());
        if !seen.insert((id, role.clone())) {
            continue;
        }
        result.push(StaffMember {
            id: id.to_string(),
            name,
            alternative_names: Vec::new(),
            role,
            image: selection.select("img").nodes().first().and_then(image_url),
        });
    }
    result
}

fn staff_tables(document: &Document) -> Vec<NodeRef<'_>> {
    let Some(anchor) = document
        .select("a[name='staff'], a#staff")
        .nodes()
        .first()
        .cloned()
    else {
        return Vec::new();
    };
    let mut tables = Vec::new();
    if let Some(next) = anchor
        .parent()
        .and_then(|parent| parent.next_element_sibling())
    {
        tables.extend(tables_until_next_heading(next));
    }
    if let Some(next) = anchor.next_element_sibling() {
        tables.extend(tables_until_next_heading(next));
    }
    tables
}

fn staff_tables_from_heading(document: &Document) -> Vec<NodeRef<'_>> {
    let Some(heading) = document
        .select("h2")
        .nodes()
        .iter()
        .cloned()
        .find(|node| clean_text(&node.text()).eq_ignore_ascii_case("Staff"))
    else {
        return Vec::new();
    };
    let mut tables = Vec::new();
    if let Some(next) = heading
        .parent()
        .and_then(|parent| parent.next_element_sibling())
    {
        tables.extend(tables_until_next_heading(next));
    }
    if let Some(next) = heading.next_element_sibling() {
        tables.extend(tables_until_next_heading(next));
    }
    tables
}

fn tables_until_next_heading(mut node: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    let mut tables = Vec::new();
    loop {
        if node.has_name("table") {
            tables.push(node.clone());
        } else if !node.is_text() {
            break;
        }
        let Some(next) = node.next_element_sibling() else {
            break;
        };
        node = next;
    }
    tables
}

fn parse_list_character(table: &NodeRef<'_>) -> Option<Character> {
    let selection = Selection::from(table.clone());
    let name_node = selection
        .select("h3.h3_character_name")
        .nodes()
        .first()?
        .clone();
    let name = node_text(&name_node)?;
    let link = Selection::from(name_node.clone())
        .select("a")
        .nodes()
        .first()
        .cloned()
        .or_else(|| name_node.parent())?;
    let href = link.attr("href")?.to_string();
    let id = id_from_href(&href, "character")?;
    let image = selection
        .select("td:first-child img")
        .nodes()
        .first()
        .and_then(image_url);
    let mut voices: Vec<(String, StaffRef)> = Vec::new();
    for row in selection
        .select("tr.js-anime-character-va-lang")
        .nodes()
        .iter()
        .cloned()
    {
        let row_selection = Selection::from(row);
        let language = {
            let value = clean_text(&row_selection.select(".js-anime-character-language").text());
            if value.is_empty() {
                clean_text(&row_selection.select("small").text())
            } else {
                value
            }
        };
        let Some(person) = person_link(&row_selection) else {
            continue;
        };
        let Some(person_id) = person
            .attr("href")
            .and_then(|href| id_from_href(&href.to_string(), "people"))
        else {
            continue;
        };
        let Some(person_name) = node_text(&person) else {
            continue;
        };
        voices.push((
            language,
            StaffRef {
                id: person_id.to_string(),
                name: person_name,
                alternative_names: Vec::new(),
                image: row_selection
                    .select("img")
                    .nodes()
                    .first()
                    .and_then(image_url),
            },
        ));
    }
    Some(Character {
        id: id.to_string(),
        tracker_data: tracker_links(&id.to_string()),
        name,
        alternative_names: Vec::new(),
        image,
        seiyuu: preferred_voices(voices),
    })
}

fn parse_detail_character(table: &NodeRef<'_>) -> Option<Character> {
    let selection = Selection::from(table.clone());
    let name_node = selection
        .select("h3.h3_characters_voice_actors")
        .nodes()
        .first()?
        .clone();
    let name = node_text(&name_node)?;
    let link = Selection::from(name_node.clone())
        .select("a")
        .nodes()
        .first()
        .cloned()
        .or_else(|| name_node.parent())?;
    let href = link.attr("href")?.to_string();
    let id = id_from_href(&href, "character")?;
    let image = selection
        .select("td:first-child img")
        .nodes()
        .first()
        .and_then(image_url);
    let mut voices = Vec::new();
    for cell in selection.select("td.va-t").nodes().iter().cloned() {
        let cell_selection = Selection::from(cell);
        let language = clean_text(&cell_selection.select("small").text());
        let Some(person) = person_link(&cell_selection) else {
            continue;
        };
        let Some(person_id) = person
            .attr("href")
            .and_then(|href| id_from_href(&href.to_string(), "people"))
        else {
            continue;
        };
        let Some(person_name) = node_text(&person) else {
            continue;
        };
        voices.push((
            language,
            StaffRef {
                id: person_id.to_string(),
                name: person_name,
                alternative_names: Vec::new(),
                image: cell_selection
                    .select("img")
                    .nodes()
                    .first()
                    .and_then(image_url),
            },
        ));
    }
    Some(Character {
        id: id.to_string(),
        tracker_data: tracker_links(&id.to_string()),
        name,
        alternative_names: Vec::new(),
        image,
        seiyuu: preferred_voices(voices),
    })
}

/// Japanese cast first; fall back to every language if MAL has no Japanese row.
fn preferred_voices(voices: Vec<(String, StaffRef)>) -> Vec<StaffRef> {
    let has_japanese = voices
        .iter()
        .any(|(language, _)| language.eq_ignore_ascii_case("Japanese"));
    let mut seen = HashSet::new();
    voices
        .into_iter()
        .filter(|(language, _)| !has_japanese || language.eq_ignore_ascii_case("Japanese"))
        .filter_map(|(_, person)| seen.insert(person.id.clone()).then_some(person))
        .collect()
}

fn person_link<'a>(selection: &Selection<'a>) -> Option<NodeRef<'a>> {
    selection
        .select("a[href*='/people/']")
        .nodes()
        .iter()
        .cloned()
        .find(|link| {
            id_from_href(
                &link
                    .attr("href")
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                "people",
            )
            .is_some()
                && !Selection::from(link.clone()).select("img").exists()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::document;

    #[test]
    fn roles_come_from_the_existing_cast_page_and_missing_roles_are_unknown() {
        let fixture = include_str!("../../tests/fixtures/characters.html");
        let main = characters_with_roles(&document(fixture));
        assert_eq!(main[0].role.as_deref(), Some("main"));
        assert!(main.iter().all(|entry| entry.character.seiyuu.is_empty()));
        let supporting = characters_with_roles(&document(&fixture.replace("Main&nbsp;", "Supporting")));
        assert_eq!(supporting[0].role.as_deref(), Some("supporting"));
        let unknown = characters_with_roles(&document(&fixture.replace("Main&nbsp;", "Unknown")));
        assert!(unknown[0].role.is_none());
    }

    #[test]
    fn anime_characters_prefer_japanese_cast_and_deduplicate() {
        let characters = characters(&document(include_str!(
            "../../tests/fixtures/characters.html"
        )));
        assert_eq!(characters.len(), 3);
        let sakura = characters.iter().find(|item| item.id == "145").unwrap();
        assert_eq!(sakura.name, "Haruno, Sakura");
        assert_eq!(sakura.seiyuu.len(), 1);
        assert_eq!(sakura.seiyuu[0].name, "Nakamura, Chie");
        assert_eq!(sakura.tracker_data[0].name, "MyAnimeList");
        assert_eq!(sakura.tracker_data[0].data, "145");
        let sasuke = characters.iter().find(|item| item.id == "13").unwrap();
        assert!(sasuke.seiyuu.is_empty());
    }

    #[test]
    fn staff_roles_are_kept_and_duplicates_are_removed() {
        let staff = staff(&document(include_str!(
            "../../tests/fixtures/characters.html"
        )));
        assert_eq!(staff.len(), 2);
        assert_eq!(staff[0].name, "Kishimoto, Masashi");
        assert_eq!(staff[0].role, "Original Creator");
        assert_eq!(
            staff.iter().find(|item| item.id == "16407").unwrap().role,
            "Producer"
        );
    }

    #[test]
    fn manga_character_layout_has_no_voice_actors() {
        let document = document(
            r#"<html><body><div class="manga-character-container"><table class="js-manga-character-table">
            <tr><td><div class="picSurround"><a href="/character/423/Casca"><img data-src="https://cdn.myanimelist.net/r/42x62/images/characters/14/72697.jpg"></a></div></td>
            <td><a href="/character/423/Casca"><h3 class="h3_character_name">Casca</h3></a><div class="spaceit_pad"><small>Main</small></div></td></tr>
            </table></div></body></html>"#,
        );
        let characters = characters(&document);
        assert_eq!(characters.len(), 1);
        assert_eq!(characters[0].id, "423");
        assert!(characters[0].seiyuu.is_empty());
    }

    #[test]
    fn detail_fallback_reads_embedded_character_section() {
        let document = document(
            r#"<html><body><div class="detail-characters-list">
            <div class="left-column"><table>
              <tr><td valign="top" width="27"><div class="picSurround"><a href="/character/17/Naruto_Uzumaki"><img data-src="https://cdn.myanimelist.net/r/42x62/images/characters/2/284121.webp"></a></div></td>
              <td valign="top"><h3 class="h3_characters_voice_actors"><a href="/character/17/Naruto_Uzumaki">Uzumaki, Naruto</a></h3><div class="spaceit_pad"><small>Main</small></div></td>
              <td align="right"><table><tr><td class="va-t ar pl4 pr4"><a href="/people/15/Junko_Takeuchi">Takeuchi, Junko</a><br><small>Japanese</small></td><td><img data-src="https://cdn.myanimelist.net/r/42x62/images/voiceactors/1/54677.jpg"></td></tr></table></td></tr>
            </table></div></div></body></html>"#,
        );
        let characters = characters_from_detail(&document);
        assert_eq!(characters.len(), 1);
        assert_eq!(characters[0].seiyuu[0].name, "Takeuchi, Junko");
    }
}
