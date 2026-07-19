use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum AtomicWriteError {
    #[error("failed to create temp file at {target}")]
    CreateTemp { target: PathBuf, source: std::io::Error },
    #[error("failed to write to temp file for {target}")]
    Write { target: PathBuf, source: std::io::Error },
    #[error("failed to flush temp file for {target}")]
    Flush { target: PathBuf, source: std::io::Error },
    #[error("failed to sync temp file for {target}")]
    SyncFile { target: PathBuf, source: std::io::Error },
    #[error("failed to replace {target}")]
    Replace { target: PathBuf, source: std::io::Error },
    #[error("sharing violation: {target} is locked by another process")]
    SharingViolation { target: PathBuf },
    #[error("failed to sync parent directory of {target}")]
    SyncParent { target: PathBuf, source: std::io::Error },
    #[error("failed to clean up temp file for {target}")]
    Cleanup { target: PathBuf, source: std::io::Error },
}

pub(crate) trait AtomicReplaceBackend: Send + Sync {
    fn create_unique_temp(&self, target: &Path) -> Result<(PathBuf, File), AtomicWriteError>;
    fn replace_existing(&self, temp: &Path, target: &Path) -> Result<(), AtomicWriteError>;
    fn sync_parent(&self, parent: &Path) -> Result<(), AtomicWriteError>;
    fn cleanup_temp(&self, temp: &Path);
}

fn do_atomic_replace(path: &Path, bytes: &[u8], backend: &dyn AtomicReplaceBackend) -> Result<(), AtomicWriteError> {
    let (temp_path, mut file) = backend.create_unique_temp(path)?;
    let result = (|| -> Result<(), AtomicWriteError> {
        file.write_all(bytes).map_err(|e| AtomicWriteError::Write {
            target: path.to_path_buf(),
            source: e,
        })?;
        file.flush().map_err(|e| AtomicWriteError::Flush {
            target: path.to_path_buf(),
            source: e,
        })?;
        file.sync_all().map_err(|e| AtomicWriteError::SyncFile {
            target: path.to_path_buf(),
            source: e,
        })?;
        drop(file);
        backend.replace_existing(&temp_path, path)?;
        if let Some(parent) = path.parent() {
            backend.sync_parent(parent)?;
        }
        Ok(())
    })();
    if result.is_err() {
        backend.cleanup_temp(&temp_path);
    }
    result
}

/// Atomically replace file contents.
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), AtomicWriteError> {
    do_atomic_replace(path, bytes, &UnixBackend)
}

#[cfg(test)]
pub(crate) fn atomic_replace_test(
    path: &Path,
    bytes: &[u8],
    backend: &dyn AtomicReplaceBackend,
) -> Result<(), AtomicWriteError> {
    do_atomic_replace(path, bytes, backend)
}

// ── Unix backend (P3-007) ──

struct UnixBackend;

impl AtomicReplaceBackend for UnixBackend {
    fn create_unique_temp(&self, target: &Path) -> Result<(PathBuf, File), AtomicWriteError> {
        let dir = target.parent().unwrap_or(Path::new("."));
        let mut err = None;
        for _ in 0..10 {
            let temp = dir.join(format!(".tmp_{}", std::process::id()));
            match std::fs::OpenOptions::new().create_new(true).write(true).open(&temp) {
                Ok(f) => return Ok((temp, f)),
                Err(e) => err = Some(e),
            }
        }
        Err(AtomicWriteError::CreateTemp {
            target: target.to_path_buf(),
            source: err.unwrap_or_else(|| std::io::Error::other("too many collisions")),
        })
    }

    fn replace_existing(&self, temp: &Path, target: &Path) -> Result<(), AtomicWriteError> {
        std::fs::rename(temp, target).map_err(|e| AtomicWriteError::Replace {
            target: target.to_path_buf(),
            source: e,
        })
    }

    fn sync_parent(&self, parent: &Path) -> Result<(), AtomicWriteError> {
        let dir = File::open(parent).map_err(|e| AtomicWriteError::SyncParent {
            target: parent.to_path_buf(),
            source: e,
        })?;
        dir.sync_all().map_err(|e| AtomicWriteError::SyncParent {
            target: parent.to_path_buf(),
            source: e,
        })
    }

    fn cleanup_temp(&self, temp: &Path) {
        let _ = std::fs::remove_file(temp);
    }
}

// ── Windows backend (P3-008) ──

// ── Deterministic fake backend for contract tests ──

#[cfg(test)]
pub(crate) struct FakeBackend {
    create_count: Mutex<usize>,
    fail_create: Mutex<bool>,
    fail_replace: Mutex<bool>,
    fail_sync_parent: Mutex<bool>,
}

#[cfg(test)]
impl FakeBackend {
    pub(crate) fn new() -> Self {
        Self {
            create_count: Mutex::new(0),
            fail_create: Mutex::new(false),
            fail_replace: Mutex::new(false),
            fail_sync_parent: Mutex::new(false),
        }
    }
}

#[cfg(test)]
impl AtomicReplaceBackend for FakeBackend {
    fn create_unique_temp(&self, target: &Path) -> Result<(PathBuf, File), AtomicWriteError> {
        if *self.fail_create.lock().unwrap() {
            return Err(AtomicWriteError::CreateTemp {
                target: target.to_path_buf(),
                source: std::io::Error::other("injected create failure"),
            });
        }
        let mut count = self.create_count.lock().unwrap();
        *count += 1;
        let dir = target.parent().unwrap_or(Path::new("."));
        let temp = dir.join(format!("__test_{}_{}", std::process::id(), count));
        File::create_new(&temp).map(|f| (temp, f)).map_err(|e| AtomicWriteError::CreateTemp {
            target: target.to_path_buf(),
            source: e,
        })
    }

    fn replace_existing(&self, temp: &Path, target: &Path) -> Result<(), AtomicWriteError> {
        if *self.fail_replace.lock().unwrap() {
            return Err(AtomicWriteError::Replace {
                target: target.to_path_buf(),
                source: std::io::Error::other("injected replace failure"),
            });
        }
        std::fs::rename(temp, target).map_err(|e| AtomicWriteError::Replace {
            target: target.to_path_buf(),
            source: e,
        })
    }

    fn sync_parent(&self, _parent: &Path) -> Result<(), AtomicWriteError> {
        if *self.fail_sync_parent.lock().unwrap() {
            return Err(AtomicWriteError::SyncParent {
                target: _parent.to_path_buf(),
                source: std::io::Error::other("injected sync failure"),
            });
        }
        Ok(())
    }

    fn cleanup_temp(&self, temp: &Path) {
        let _ = std::fs::remove_file(temp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_backend() -> FakeBackend {
        FakeBackend::new()
    }

    #[test]
    fn atomic_replace_success() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("test.txt");
        let backend = test_backend();

        atomic_replace_test(&target, b"hello", &backend).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn atomic_replace_unicode_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("unicode.txt");
        let backend = test_backend();

        atomic_replace_test(&target, "héllo wörld 🌍".as_bytes(), &backend).unwrap();
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "héllo wörld 🌍"
        );
    }

    #[test]
    fn atomic_replace_path_with_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("my file.txt");
        let backend = test_backend();

        atomic_replace_test(&target, b"spaces", &backend).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "spaces");
    }

    #[test]
    fn atomic_replace_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("new_file.txt");
        let backend = test_backend();

        assert!(!target.exists());
        atomic_replace_test(&target, b"new", &backend).unwrap();
        assert!(target.exists());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn atomic_replace_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("existing.txt");
        std::fs::write(&target, b"old").unwrap();

        atomic_replace_test(&target, b"new", &test_backend()).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn atomic_replace_failure_does_not_corrupt_original() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("protected.txt");
        std::fs::write(&target, b"original").unwrap();

        let backend = test_backend();
        *backend.fail_replace.lock().unwrap() = true;

        let result = atomic_replace_test(&target, b"replacement", &backend);
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "original");
    }

    #[test]
    fn atomic_replace_create_failure_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nope.txt");

        let backend = test_backend();
        *backend.fail_create.lock().unwrap() = true;

        let result = atomic_replace_test(&target, b"data", &backend);
        assert!(result.is_err());
        assert!(!target.exists());
    }

    #[test]
    fn replace_existing_old_target_preserved_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("preserve.txt");
        std::fs::write(&target, b"original").unwrap();

        let backend = test_backend();
        *backend.fail_replace.lock().unwrap() = true;

        let (temp, _) = backend.create_unique_temp(&target).unwrap();
        std::fs::write(&temp, b"new").unwrap();

        let result = backend.replace_existing(&temp, &target);
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "original");
    }

    #[test]
    fn error_contains_target_path() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("err_test.txt");

        let backend = test_backend();
        *backend.fail_create.lock().unwrap() = true;

        let err = atomic_replace_test(&target, b"x", &backend).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains(target.to_str().unwrap()),
            "error should mention target path: {err_str}"
        );
    }
}
