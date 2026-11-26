wit_bindgen::generate!({ path: "../../vendor/sdk/wit", world: "player-module" });
#[path = "../../player-embed-common/mod.rs"]
mod common;
use animex::module::types::{Dub, PlayerCapabilities, StreamData, StreamIframe};
use exports::animex::module::player_api::Guest;
struct Component;
impl Guest for Component {
    fn describe() -> Result<PlayerCapabilities, String> {
        Ok(PlayerCapabilities {
            qualities: vec![],
            protocols: vec!["cvh.embed.v1".into()],
            languages: vec!["ru".into()],
        })
    }
    fn get_dubs(payload: String) -> Result<Vec<Dub>, String> {
        common::dubs(&payload)
    }
    fn get_link(
        payload: String,
        dub_id: String,
        _quality: Option<String>,
    ) -> Result<StreamData, String> {
        let payload = common::parse(&payload)?;
        let variant = common::selected(&payload, &dub_id)?;
        if let Some(context) = &variant.cvh {
            return cvh::resolve(context, payload.referer.as_deref());
        }
        Ok(StreamData::Iframe(StreamIframe {
            url: variant.url.clone(),
        }))
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
        match Component::get_link(payload.into(), "dub-b".into(), None).unwrap() {
            StreamData::Iframe(value) => assert_eq!(value.url, "https://player.example/b"),
            _ => panic!("expected iframe"),
        }
        assert!(Component::get_link(payload.into(), "missing".into(), None).is_err());
    }
}

mod cvh;
