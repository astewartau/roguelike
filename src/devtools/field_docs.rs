//! What each component field means, for the developer tools' (?) hints,
//! read from the doc comments on the fields in `src/components.rs` itself.
//!
//! The source is embedded at compile time and parsed once, so the hints
//! are the same words the code is documented with and can't drift from
//! it. A field with no doc comment simply has no hint.

use std::collections::HashMap;
use std::sync::OnceLock;

const SOURCE: &str = include_str!("../components.rs");

struct Docs {
    /// (struct, field) -> doc.
    by_struct: HashMap<(String, String), String>,
    /// field -> doc, for field names documented the same way everywhere
    /// they appear (so a field of a nested value can be found without
    /// knowing its type).
    by_field: HashMap<String, Option<String>>,
}

fn docs() -> &'static Docs {
    static DOCS: OnceLock<Docs> = OnceLock::new();
    DOCS.get_or_init(|| parse(SOURCE))
}

fn parse(source: &str) -> Docs {
    let mut by_struct = HashMap::new();
    let mut by_field: HashMap<String, Option<String>> = HashMap::new();
    let mut current: Option<String> = None;
    let mut pending: Vec<String> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("pub struct ") {
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            current = rest.contains('{').then_some(name);
            pending.clear();
            continue;
        }
        if line.starts_with('}') {
            current = None;
            pending.clear();
            continue;
        }
        if let Some(doc) = trimmed.strip_prefix("///") {
            pending.push(doc.trim().to_string());
            continue;
        }
        if trimmed.starts_with("#[") {
            continue;
        }
        if let (Some(owner), Some(rest)) = (&current, trimmed.strip_prefix("pub ")) {
            if let Some((field, _)) = rest.split_once(':') {
                let field = field.trim().to_string();
                if !pending.is_empty() {
                    let doc = pending.join(" ");
                    by_struct.insert((owner.clone(), field.clone()), doc.clone());
                    by_field
                        .entry(field)
                        .and_modify(|seen| {
                            if seen.as_deref() != Some(doc.as_str()) {
                                *seen = None;
                            }
                        })
                        .or_insert(Some(doc));
                }
            }
        }
        pending.clear();
    }
    Docs { by_struct, by_field }
}

/// What `field` of `component` means, if it's documented.
pub fn doc(component: &str, field: &str) -> Option<&'static str> {
    let docs = docs();
    docs.by_struct
        .get(&(component.to_string(), field.to_string()))
        .or_else(|| docs.by_field.get(field).and_then(|d| d.as_ref()))
        .map(String::as_str)
}

/// A field name as a person would write it: "last_regen_time" becomes
/// "Last regen time".
pub fn label(field: &str) -> String {
    let words = field.replace('_', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_documented_from_the_source() {
        let radius = doc("LightSource", "radius").expect("LightSource.radius is documented");
        assert!(radius.to_lowercase().contains("radius"), "{radius}");
        assert!(doc("Channel", "breaks_on_damage").is_some_and(|d| d.contains("damage")));
        assert_eq!(doc("Position", "no_such_field"), None);
        assert_eq!(label("last_regen_time"), "Last regen time");
    }
}
