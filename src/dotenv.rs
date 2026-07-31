use indexmap::IndexMap;
use std::path::Path;

/// Result of parsing a single `.env` file. Never hard-fails: an unreadable
/// or malformed file yields an empty (or partial) map plus warnings, so one
/// bad layer never blocks discovery of sibling layers.
pub struct ParsedEnv {
    pub vars: IndexMap<String, String>,
    pub warnings: Vec<String>,
}

pub fn parse_file(path: &Path) -> ParsedEnv {
    let iter = match dotenvy::from_path_iter(path) {
        Ok(iter) => iter,
        Err(e) => {
            return ParsedEnv {
                vars: IndexMap::new(),
                warnings: vec![format!("{}: {}", path.display(), e)],
            };
        }
    };

    let mut vars = IndexMap::new();
    let mut warnings = Vec::new();
    for item in iter {
        match item {
            Ok((k, v)) => {
                vars.insert(k, v);
            }
            Err(e) => {
                warnings.push(format!("{}: {}", path.display(), e));
            }
        }
    }
    ParsedEnv { vars, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_fixture(contents: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parses_basic_key_value() {
        let f = write_fixture("FOO=bar\n");
        let parsed = parse_file(f.path());
        assert_eq!(parsed.vars.get("FOO"), Some(&"bar".to_string()));
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn parses_quotes_comments_export_prefix_blank_lines() {
        let f = write_fixture(
            "# a comment\n\nexport FOO=\"hello world\"\nBAR='literal $NOT_EXPANDED'\n",
        );
        let parsed = parse_file(f.path());
        assert_eq!(parsed.vars.get("FOO"), Some(&"hello world".to_string()));
        assert_eq!(
            parsed.vars.get("BAR"),
            Some(&"literal $NOT_EXPANDED".to_string())
        );
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn skips_malformed_line_but_keeps_valid_ones() {
        let f = write_fixture("FOO=1\nnot a valid line\nBAR=2\n");
        let parsed = parse_file(f.path());
        assert_eq!(parsed.vars.get("FOO"), Some(&"1".to_string()));
        assert_eq!(parsed.vars.get("BAR"), Some(&"2".to_string()));
        assert!(!parsed.warnings.is_empty());
    }

    #[test]
    fn missing_file_yields_empty_with_warning() {
        let parsed = parse_file(Path::new("/nonexistent/path/.env"));
        assert!(parsed.vars.is_empty());
        assert!(!parsed.warnings.is_empty());
    }
}
