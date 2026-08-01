use crate::config::{self, Config};
use std::path::{Path, PathBuf};

/// Safety valve against pathological parent chains; real filesystems never
/// come close to this depth via `Path::parent()`.
const MAX_DEPTH: usize = 512;

/// Walk from `start` up to the filesystem root, collecting every directory
/// that contains a `.env` file. Returned root-first (parent before child)
/// so callers can fold left-to-right with later entries overriding earlier
/// ones. `start` is canonicalized first so symlinked working directories
/// resolve to their real path.
///
/// A directory matched by `config.is_skipped_dir` or with a vendored
/// component (`node_modules`, `.venv`, etc.) is not collected *from* --
/// the walk still continues past it upward, since a skipped directory in
/// the middle of the chain shouldn't block a legitimate `.env` further up.
pub fn discover_env_files(start: &Path, config: &Config) -> Vec<PathBuf> {
    let canonical = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    let mut found = Vec::new();
    let mut dir = Some(canonical);
    let mut depth = 0;
    while let Some(d) = dir {
        if depth >= MAX_DEPTH {
            break;
        }
        if !config.is_skipped_dir(&d) && !config::has_vendored_component(&d) {
            let candidate = d.join(".env");
            if candidate.is_file() {
                found.push(candidate);
            }
        }
        dir = d.parent().map(Path::to_path_buf);
        depth += 1;
    }
    found.reverse();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // These fixtures land under `/tmp` (via `tempfile`), which the real
    // compiled-in defaults deliberately skip -- so tests of plain walk
    // mechanics use `Config::unrestricted()` to avoid failing for a
    // reason unrelated to what they're checking. Tests of the skip-list
    // itself use `defaults()`/`unrestricted_with_skip` explicitly.

    #[test]
    fn discovers_root_first_order() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let child = parent.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(parent.join(".env"), "FOO=parent\n").unwrap();
        fs::write(child.join(".env"), "FOO=child\n").unwrap();

        let files = discover_env_files(&child, &Config::unrestricted());
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], fs::canonicalize(&parent).unwrap().join(".env"));
        assert_eq!(files[1], fs::canonicalize(&child).unwrap().join(".env"));
    }

    #[test]
    fn no_env_files_yields_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path().join("a/b/c");
        fs::create_dir_all(&leaf).unwrap();
        let files = discover_env_files(&leaf, &Config::unrestricted());
        assert!(files.is_empty());
    }

    #[test]
    fn skips_directories_without_env_between_ancestors() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = a.join("b");
        let c = b.join("c");
        fs::create_dir_all(&c).unwrap();
        fs::write(a.join(".env"), "FOO=a\n").unwrap();
        // no .env in b or c
        let files = discover_env_files(&c, &Config::unrestricted());
        assert_eq!(files, vec![fs::canonicalize(&a).unwrap().join(".env")]);
    }

    #[test]
    fn vendored_component_is_skipped_but_walk_continues_past_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        let vendored = root.join("node_modules").join("some-pkg");
        fs::create_dir_all(&vendored).unwrap();
        fs::write(root.join(".env"), "FOO=root\n").unwrap();
        fs::write(vendored.join(".env"), "FOO=vendored\n").unwrap();

        // has_vendored_component is unconditional (not config-driven), so
        // this uses `unrestricted()` to isolate it from the compiled-in
        // skip list.
        let files = discover_env_files(&vendored, &Config::unrestricted());
        // Only the root .env is collected -- the vendored directory's own
        // .env is skipped, but the walk still continues upward past it.
        assert_eq!(files, vec![fs::canonicalize(&root).unwrap().join(".env")]);
    }

    #[test]
    fn configured_skip_extra_directory_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let skipped = tmp.path().join("skip-me");
        fs::create_dir_all(&skipped).unwrap();
        fs::write(skipped.join(".env"), "FOO=1\n").unwrap();
        let canonical_skipped = fs::canonicalize(&skipped).unwrap();

        // The actual TOML round-trip for skip_extra is already covered by
        // config.rs's own tests; this only needs to verify that
        // discover_env_files respects Config::is_skipped_dir.
        let config = Config::unrestricted_with_skip(&[canonical_skipped]);

        let files = discover_env_files(&skipped, &config);
        assert!(files.is_empty());
    }

    #[test]
    fn compiled_in_defaults_skip_tmp_itself() {
        // Sanity check the real, non-test-overridden defaults against the
        // exact scenario the other tests in this file work around: a
        // fixture under /tmp is not collected when using real defaults.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(".env"), "FOO=1\n").unwrap();
        let files = discover_env_files(tmp.path(), &Config::defaults());
        assert!(files.is_empty());
    }
}
