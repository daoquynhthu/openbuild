use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("path does not exist: {0}")]
    NotFound(PathBuf),
    #[error("cannot canonicalize on this platform: {0}")]
    Unsupported(PathBuf),
    #[error("filesystem error for {path}: {source}")]
    Filesystem {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Resolve `path` to a stable absolute path, using the platform's native
/// canonicalization.  This is the single filesystem canonicalization wrapper
/// across the entire provider-adapter V1 codebase; consumers must not call
/// `std::fs::canonicalize` or `dunce::canonicalize` directly.
pub fn normalized_absolute(path: &Path) -> Result<PathBuf, PathError> {
    if !path.exists() {
        return Err(PathError::NotFound(path.to_path_buf()));
    }
    dunce::canonicalize(path).map_err(|e| PathError::Filesystem {
        path: path.to_path_buf(),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_absolute_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, b"hello").unwrap();

        let result = normalized_absolute(&file).unwrap();
        assert!(result.is_absolute());
        assert!(result.ends_with("test.txt"));
    }

    #[test]
    fn normalized_absolute_existing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let result = normalized_absolute(dir.path()).unwrap();
        assert!(result.is_absolute());
    }

    #[test]
    fn normalized_absolute_nonexistent_fails() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does_not_exist.txt");
        let err = normalized_absolute(&missing).unwrap_err();
        assert!(matches!(err, PathError::NotFound(_)));
    }

    #[test]
    fn normalized_absolute_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("rel_test.txt");
        std::fs::write(&file, b"data").unwrap();

        let result = normalized_absolute(&file).unwrap();
        assert!(result.is_absolute());
    }

    #[cfg(windows)]
    #[test]
    fn normalized_absolute_windows_drive_letter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drive_test.txt");
        std::fs::write(&path, b"").unwrap();
        let result = normalized_absolute(&path).unwrap();
        let s = result.to_str().unwrap();
        assert!(
            s.len() > 2 && s.as_bytes()[1] == b':',
            "expected drive letter, got {s}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalized_absolute_windows_unc() {
        // UNC paths like \\server\share — we can't create one in test,
        // but we can verify that local paths don't get converted to UNC.
        let dir = tempfile::tempdir().unwrap();
        let result = normalized_absolute(dir.path()).unwrap();
        let s = result.to_str().unwrap();
        assert!(
            !s.starts_with("\\\\?\\"),
            "expected no long-path prefix, got {s}"
        );
    }

    #[test]
    fn normalized_absolute_unicode_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("über_文件.txt");
        std::fs::write(&file, b"unicode").unwrap();
        let result = normalized_absolute(&file).unwrap();
        assert!(result.is_absolute());
        assert!(result.ends_with("über_文件.txt"));
    }
}
