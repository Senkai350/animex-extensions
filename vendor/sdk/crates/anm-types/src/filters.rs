//! Filter fields and choice presentation hints.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionPair {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChoicePresentation {
    Dropdown,
    CheckboxSelect,
    RadioSelect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FilterField {
    Sort {
        id: String,
        label: String,
        #[serde(default)]
        options: Vec<OptionPair>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        #[serde(default)]
        directions: Vec<OptionPair>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(rename = "defaultDirection")]
        default_direction: Option<String>,
    },
    Select {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presentation: Option<ChoicePresentation>,
        label: String,
        #[serde(default)]
        options: Vec<OptionPair>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    Multiselect {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presentation: Option<ChoicePresentation>,
        label: String,
        #[serde(default)]
        options: Vec<OptionPair>,
        #[serde(default)]
        default: Vec<String>,
    },
    Range {
        id: String,
        label: String,
        min: f32,
        max: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<(f32, f32)>,
    },
    Toggle {
        id: String,
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
    },
    Text {
        id: String,
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::{FilterField, OptionPair};

    #[test]
    fn sort_field_keeps_the_ipc_contract_shape() {
        let field = FilterField::Sort {
            id: "sort".into(),
            label: "Sort by".into(),
            options: vec![OptionPair {
                value: "title".into(),
                label: "Title".into(),
            }],
            default: Some("title".into()),
            directions: vec![OptionPair {
                value: "asc".into(),
                label: "Ascending".into(),
            }],
            default_direction: Some("asc".into()),
        };
        let json = serde_json::to_value(field).expect("serialize sort field");
        assert_eq!(json["type"], "sort");
        assert_eq!(json["defaultDirection"], "asc");
        assert_eq!(json["options"][0]["value"], "title");
    }
}
