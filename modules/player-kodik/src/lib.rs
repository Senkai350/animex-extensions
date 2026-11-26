wit_bindgen::generate!({ path: "../../vendor/sdk/wit", world: "player-module" });
#[path = "../../player-embed-common/mod.rs"]
mod common;
use animex::module::types::{Dub, PlayerCapabilities, StreamData};
use exports::animex::module::player_api::Guest;
mod resolve;
struct Component;
impl Guest for Component {
    fn describe() -> Result<PlayerCapabilities, String> {
        Ok(PlayerCapabilities {
            qualities: vec![],
            protocols: vec!["kodik.embed.v1".into()],
            languages: vec!["ru".into()],
        })
    }
    fn get_dubs(payload: String) -> Result<Vec<Dub>, String> {
        common::dubs(&payload)
    }
    fn get_link(
        payload: String,
        dub_id: String,
        quality: Option<String>,
    ) -> Result<StreamData, String> {
        let payload = common::parse(&payload)?;
        let variant = common::selected(&payload, &dub_id)?;

        resolve::resolve(&variant.url, payload.referer.as_deref(), quality.as_deref())
    }
}
export!(Component);
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn another_source_can_select_each_dub() {
        let payload = r#"{"referer":"https://other.example/","variants":[{"id":"dub-a","label":"A","url":"https://player.example/a"},{"id":"dub-b","label":"B","url":"https://player.example/b","language":"ja"}]}"#;
        let dubs = Component::get_dubs(payload.into()).unwrap();
        assert_eq!(dubs.len(), 2);
        assert_eq!(dubs[1].language.as_deref(), Some("ja"));
        assert!(dubs[0].qualities.is_empty());
        let decoded = common::parse(payload).unwrap();
        assert_eq!(
            common::selected(&decoded, "dub-b").unwrap().url,
            "https://player.example/b"
        );
        assert!(Component::get_link(payload.into(), "missing".into(), None).is_err());
    }
}
