//! Canonical JSON (a JCS subset sufficient for the manifest).

use crate::model::Manifest;

/// Canonical JSON: sorted object keys, no insignificant whitespace.
///
/// This is a subset of JCS (RFC 8785) sufficient for the manifest: keys are
/// sorted bytewise by UTF-8, strings use JCS escaping, and numbers in the
/// manifest are integers only (`api_version`), so `serde_json::Number` is
/// stable. Full float normalization is intentionally not implemented.
pub fn canonical_json(value: &serde_json::Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Null => out.push_str("null"),
        serde_json::Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        serde_json::Value::Number(n) => out.push_str(&n.to_string()),
        serde_json::Value::String(s) => write_canonical_string(s, out),
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical_string(key, out);
                out.push(':');
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
    }
}

fn write_canonical_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0C}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Manifest {
    /// Canonical form of the manifest without the `signature` field.
    pub fn canonical_unsigned(&self) -> String {
        let mut value = serde_json::to_value(self).expect("manifest is serializable");
        if let serde_json::Value::Object(map) = &mut value {
            map.remove("signature");
        }
        canonical_json(&value)
    }
}
