use std::path::{Path, PathBuf};
use std::time::SystemTime;
use xxhash_rust::xxh3::Xxh3;

/// Cheap signature over the current directory plus the mtime/size of every
/// discovered `.env` file, used to skip discovery/parse/merge/diff entirely
/// when nothing has changed since the last invocation (the dominant case:
/// sitting at a prompt, nothing changed). Uses a fast non-cryptographic hash
/// -- this is a hot path, not a security boundary.
///
/// Correctness note: this deliberately invalidates on mtime change even if
/// content didn't actually change (e.g. a touch with no edit) rather than
/// trying to detect "content unchanged" without reading the file, which
/// would defeat the point of a fast path. Occasionally recomputing
/// unnecessarily is fine; wrongly skipping a real change is not.
pub fn compute_signature(cwd: &Path, env_files: &[PathBuf]) -> u64 {
    let mut hasher = Xxh3::new();
    hasher.update(cwd.as_os_str().as_encoded_bytes());
    for path in env_files {
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
    hasher.digest()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn same_inputs_yield_same_signature() {
        let tmp = tempfile::tempdir().unwrap();
        let env_path = tmp.path().join(".env");
        fs::write(&env_path, "FOO=1\n").unwrap();
        let files = vec![env_path.clone()];
        let sig1 = compute_signature(tmp.path(), &files);
        let sig2 = compute_signature(tmp.path(), &files);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn different_cwd_yields_different_signature() {
        let tmp = tempfile::tempdir().unwrap();
        let sig1 = compute_signature(tmp.path(), &[]);
        let sig2 = compute_signature(Path::new("/some/other/dir"), &[]);
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn touching_mtime_without_content_change_invalidates_signature() {
        let tmp = tempfile::tempdir().unwrap();
        let env_path = tmp.path().join(".env");
        fs::write(&env_path, "FOO=1\n").unwrap();
        let files = vec![env_path.clone()];
        let sig1 = compute_signature(tmp.path(), &files);

        // Bump mtime forward without changing content.
        let new_time = SystemTime::now() + std::time::Duration::from_secs(60);
        let f = fs::File::open(&env_path).unwrap();
        f.set_modified(new_time).unwrap();

        let sig2 = compute_signature(tmp.path(), &files);
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn different_file_length_yields_different_signature() {
        let tmp = tempfile::tempdir().unwrap();
        let env_path = tmp.path().join(".env");
        fs::write(&env_path, "FOO=1\n").unwrap();
        let files = vec![env_path.clone()];
        let sig1 = compute_signature(tmp.path(), &files);

        fs::write(&env_path, "FOO=12345\n").unwrap();
        let sig2 = compute_signature(tmp.path(), &files);
        assert_ne!(sig1, sig2);
    }
}
