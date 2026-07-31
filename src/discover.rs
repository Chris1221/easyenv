use std::path::{Path, PathBuf};

/// Safety valve against pathological parent chains; real filesystems never
/// come close to this depth via `Path::parent()`.
const MAX_DEPTH: usize = 512;

/// Walk from `start` up to the filesystem root, collecting every directory
/// that contains a `.env` file. Returned root-first (parent before child)
/// so callers can fold left-to-right with later entries overriding earlier
/// ones. `start` is canonicalized first so symlinked working directories
/// resolve to their real path.
pub fn discover_env_files(start: &Path) -> Vec<PathBuf> {
    let canonical = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    let mut found = Vec::new();
    let mut dir = Some(canonical);
    let mut depth = 0;
    while let Some(d) = dir {
        if depth >= MAX_DEPTH {
            break;
        }
        let candidate = d.join(".env");
        if candidate.is_file() {
            found.push(candidate);
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

    #[test]
    fn discovers_root_first_order() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let child = parent.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(parent.join(".env"), "FOO=parent\n").unwrap();
        fs::write(child.join(".env"), "FOO=child\n").unwrap();

        let files = discover_env_files(&child);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], fs::canonicalize(&parent).unwrap().join(".env"));
        assert_eq!(files[1], fs::canonicalize(&child).unwrap().join(".env"));
    }

    #[test]
    fn no_env_files_yields_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path().join("a/b/c");
        fs::create_dir_all(&leaf).unwrap();
        let files = discover_env_files(&leaf);
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
        let files = discover_env_files(&c);
        assert_eq!(files, vec![fs::canonicalize(&a).unwrap().join(".env")]);
    }
}
