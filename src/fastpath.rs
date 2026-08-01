use std::path::{Path, PathBuf};
use std::time::SystemTime;
use xxhash_rust::xxh3::Xxh3;

/// Cheap signature over the current directory plus the mtime/size of every
/// discovered `.env` file and the config file, used to skip
/// discovery/parse/merge/diff entirely when nothing has changed since the
/// last invocation (the dominant case: sitting at a prompt, nothing
/// changed). Uses a fast non-cryptographic hash -- this is a hot path, not
/// a security boundary.
///
/// `config_path` must be included: without it, editing `config.toml` (e.g.
/// adding a `deny_extra` entry) wouldn't invalidate the cached signature
/// until some unrelated `.env` change also happened to do so, so a
/// newly-denied key could keep being exported from stale state.
///
/// Correctness note: this deliberately invalidates on mtime change even if
/// content didn't actually change (e.g. a touch with no edit) rather than
/// trying to detect "content unchanged" without reading the file, which
/// would defeat the point of a fast path. Occasionally recomputing
/// unnecessarily is fine; wrongly skipping a real change is not.
pub fn compute_signature(cwd: &Path, env_files: &[PathBuf], config_path: &Path) -> u64 {
    let mut hasher = Xxh3::new();
    hasher.update(cwd.as_os_str().as_encoded_bytes());
    for path in env_files {
        hash_path_metadata(&mut hasher, path);
    }
    hash_path_metadata(&mut hasher, config_path);
    hasher.digest()
}

/// Hashes a path's bytes plus, if it exists, its mtime and length. A
/// missing file (e.g. no config.toml) still contributes its path bytes,
/// so its *absence* is part of the signature too.
fn hash_path_metadata(hasher: &mut Xxh3, path: &Path) {
    hasher.update(path.as_os_str().as_encoded_bytes());
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(modified) = meta.modified()
            && let Ok(dur) = modified.duration_since(SystemTime::UNIX_EPOCH)
        {
            hasher.update(&dur.as_nanos().to_le_bytes());
        }
        hasher.update(&meta.len().to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn no_config() -> PathBuf {
        PathBuf::from("/nonexistent/easyenv/config.toml")
    }

    #[test]
    fn same_inputs_yield_same_signature() {
        let tmp = tempfile::tempdir().unwrap();
        let env_path = tmp.path().join(".env");
        fs::write(&env_path, "FOO=1\n").unwrap();
        let files = vec![env_path.clone()];
        let sig1 = compute_signature(tmp.path(), &files, &no_config());
        let sig2 = compute_signature(tmp.path(), &files, &no_config());
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn different_cwd_yields_different_signature() {
        let tmp = tempfile::tempdir().unwrap();
        let sig1 = compute_signature(tmp.path(), &[], &no_config());
        let sig2 = compute_signature(Path::new("/some/other/dir"), &[], &no_config());
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn touching_mtime_without_content_change_invalidates_signature() {
        let tmp = tempfile::tempdir().unwrap();
        let env_path = tmp.path().join(".env");
        fs::write(&env_path, "FOO=1\n").unwrap();
        let files = vec![env_path.clone()];
        let sig1 = compute_signature(tmp.path(), &files, &no_config());

        // Bump mtime forward without changing content.
        let new_time = SystemTime::now() + std::time::Duration::from_secs(60);
        let f = fs::File::open(&env_path).unwrap();
        f.set_modified(new_time).unwrap();

        let sig2 = compute_signature(tmp.path(), &files, &no_config());
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn different_file_length_yields_different_signature() {
        let tmp = tempfile::tempdir().unwrap();
        let env_path = tmp.path().join(".env");
        fs::write(&env_path, "FOO=1\n").unwrap();
        let files = vec![env_path.clone()];
        let sig1 = compute_signature(tmp.path(), &files, &no_config());

        fs::write(&env_path, "FOO=12345\n").unwrap();
        let sig2 = compute_signature(tmp.path(), &files, &no_config());
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn editing_config_file_invalidates_signature_even_with_env_files_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let env_path = tmp.path().join(".env");
        fs::write(&env_path, "FOO=1\n").unwrap();
        let files = vec![env_path.clone()];
        let config_path = tmp.path().join("config.toml");

        // No config file yet.
        let sig1 = compute_signature(tmp.path(), &files, &config_path);

        fs::write(&config_path, "[env]\ndeny_extra = [\"FOO\"]\n").unwrap();
        let sig2 = compute_signature(tmp.path(), &files, &config_path);
        assert_ne!(
            sig1, sig2,
            "creating/editing the config file must invalidate the signature \
             even when no .env file changed, otherwise a newly denied key \
             could keep being exported from stale state"
        );
    }
}
