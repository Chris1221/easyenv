use indexmap::IndexMap;
use std::path::PathBuf;

pub struct EnvLayer {
    pub source: PathBuf,
    pub vars: IndexMap<String, String>,
}

/// Fold root-first layers into a single map; later (closer-to-cwd) layers
/// override earlier (parent) layers key-by-key. Non-conflicting keys from
/// every layer survive.
pub fn merge_layers(layers: &[EnvLayer]) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    for layer in layers {
        for (k, v) in &layer.vars {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(vars: &[(&str, &str)]) -> EnvLayer {
        EnvLayer {
            source: PathBuf::from("test"),
            vars: vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn child_overrides_parent_on_conflict() {
        let layers = vec![layer(&[("FOO", "parent")]), layer(&[("FOO", "child")])];
        let merged = merge_layers(&layers);
        assert_eq!(merged.get("FOO"), Some(&"child".to_string()));
    }

    #[test]
    fn non_conflicting_keys_from_all_layers_survive() {
        let layers = vec![
            layer(&[("FOO", "parent"), ("SHARED", "parent")]),
            layer(&[("FOO", "child")]),
        ];
        let merged = merge_layers(&layers);
        assert_eq!(merged.get("FOO"), Some(&"child".to_string()));
        assert_eq!(merged.get("SHARED"), Some(&"parent".to_string()));
    }

    #[test]
    fn empty_layers_yields_empty_map() {
        let merged = merge_layers(&[]);
        assert!(merged.is_empty());
    }
}
