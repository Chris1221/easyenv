use indexmap::IndexMap;
use std::path::{Path, PathBuf};

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

/// Which layer a resolved key's value actually came from -- the closest
/// (last, since layers are root-first) layer that defines it, matching
/// `merge_layers`'s override semantics. Used for warning/status messages
/// that need to name the source `.env` file.
pub fn origin_of<'a>(layers: &'a [EnvLayer], key: &str) -> Option<&'a Path> {
    layers
        .iter()
        .rev()
        .find(|l| l.vars.contains_key(key))
        .map(|l| l.source.as_path())
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
    fn origin_of_reports_the_closest_defining_layer() {
        let layers = vec![
            EnvLayer {
                source: PathBuf::from("/parent/.env"),
                vars: [("FOO", "parent"), ("SHARED", "parent")]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
            EnvLayer {
                source: PathBuf::from("/child/.env"),
                vars: [("FOO", "child")]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
        ];
        assert_eq!(origin_of(&layers, "FOO"), Some(Path::new("/child/.env")));
        assert_eq!(
            origin_of(&layers, "SHARED"),
            Some(Path::new("/parent/.env"))
        );
        assert_eq!(origin_of(&layers, "MISSING"), None);
    }

    #[test]
    fn empty_layers_yields_empty_map() {
        let merged = merge_layers(&[]);
        assert!(merged.is_empty());
    }
}
