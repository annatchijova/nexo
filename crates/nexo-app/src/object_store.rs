//! Content-addressed filesystem object store for artifact bytes.
//!
//! Per `docs/ARCHITECTURE.md`'s "Data ownership": object storage holds
//! original artifact bytes addressed by content hash. This module is the
//! storage-capability half of that sentence; `nexo-integrity` computes and
//! seals digests but performs no I/O by design (ADR 0003), so filesystem
//! access lives here, in the application layer that owns storage
//! capabilities per the layer table in `docs/ARCHITECTURE.md`.
//!
//! The store never trusts a caller-declared digest: `put` always hashes the
//! bytes it is given and addresses them by that hash, never by a name the
//! caller supplies. `get` re-verifies the digest on every read, so silent
//! on-disk corruption or tampering between write and read is detected
//! rather than handed back as if it were still the original artifact.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use nexo_integrity::{hash_bytes, Sha256Digest};

#[derive(Debug)]
pub enum ObjectStoreError {
    Io(io::Error),
    /// The bytes read back from storage do not hash to the digest that
    /// addresses them: the object was corrupted or tampered with after
    /// `put` wrote it.
    Corrupted {
        expected: Sha256Digest,
        actual: Sha256Digest,
    },
    NotFound(Sha256Digest),
}

impl From<io::Error> for ObjectStoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// A content-addressed store rooted at one directory. Every object lives at
/// `<root>/<first two hex chars>/<full hex digest>`, sharded so no directory
/// accumulates an unbounded flat file listing under real personal-scale use.
pub struct FilesystemObjectStore {
    root: PathBuf,
}

impl FilesystemObjectStore {
    /// Creates the root directory (and any missing parents) if it does not
    /// already exist.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, ObjectStoreError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Writes `bytes`, addressed by their own SHA-256 digest, and returns
    /// that digest. Writing the same bytes twice is idempotent: the second
    /// call is a no-op past the existence check, never a duplicate or a
    /// corrupting concurrent write, because the write path is
    /// write-to-temp-then-rename (atomic on the same filesystem).
    pub fn put(&self, bytes: &[u8]) -> Result<Sha256Digest, ObjectStoreError> {
        let digest = hash_bytes(bytes);
        let final_path = self.path_for(digest);
        if final_path.exists() {
            return Ok(digest);
        }
        let parent = final_path
            .parent()
            .expect("path_for always yields a file under a shard directory");
        fs::create_dir_all(parent)?;

        let mut temp_path = final_path.clone();
        temp_path.set_extension("tmp");
        fs::write(&temp_path, bytes)?;
        fs::rename(&temp_path, &final_path)?;
        Ok(digest)
    }

    /// Reads the object addressed by `digest`, verifying on every call that
    /// the bytes on disk still hash to `digest`.
    pub fn get(&self, digest: Sha256Digest) -> Result<Vec<u8>, ObjectStoreError> {
        let path = self.path_for(digest);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(ObjectStoreError::NotFound(digest));
            }
            Err(err) => return Err(err.into()),
        };
        let actual = hash_bytes(&bytes);
        if actual != digest {
            return Err(ObjectStoreError::Corrupted {
                expected: digest,
                actual,
            });
        }
        Ok(bytes)
    }

    pub fn contains(&self, digest: Sha256Digest) -> bool {
        self.path_for(digest).is_file()
    }

    fn path_for(&self, digest: Sha256Digest) -> PathBuf {
        let hex = digest.to_string();
        // Sha256Digest::to_string is a fixed-length hex encoding of 32
        // bytes (64 lowercase hex characters); it never contains a path
        // separator or a ".." segment, so this cannot escape `root`.
        shard_path(&self.root, &hex)
    }
}

fn shard_path(root: &Path, hex: &str) -> PathBuf {
    let shard = &hex[..2];
    root.join(shard).join(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_then_get_round_trips_exact_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemObjectStore::open(dir.path()).unwrap();
        let digest = store.put(b"evidence bytes").unwrap();
        assert_eq!(store.get(digest).unwrap(), b"evidence bytes");
    }

    #[test]
    fn put_is_idempotent_for_identical_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemObjectStore::open(dir.path()).unwrap();
        let first = store.put(b"same content").unwrap();
        let second = store.put(b"same content").unwrap();
        assert_eq!(first, second);
        assert_eq!(store.get(first).unwrap(), b"same content");
    }

    #[test]
    fn digest_is_computed_from_bytes_not_supplied_by_caller() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemObjectStore::open(dir.path()).unwrap();
        let digest_a = store.put(b"content a").unwrap();
        let digest_b = store.put(b"content b").unwrap();
        assert_ne!(digest_a, digest_b);
        assert_eq!(store.get(digest_a).unwrap(), b"content a");
        assert_eq!(store.get(digest_b).unwrap(), b"content b");
    }

    #[test]
    fn get_of_unknown_digest_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemObjectStore::open(dir.path()).unwrap();
        let unknown = hash_bytes(b"never written");
        match store.get(unknown) {
            Err(ObjectStoreError::NotFound(d)) => assert_eq!(d, unknown),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn tampering_with_stored_bytes_is_detected_on_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemObjectStore::open(dir.path()).unwrap();
        let digest = store.put(b"original bytes").unwrap();

        // Simulate on-disk corruption/tampering after the write.
        let path = shard_path(dir.path(), &digest.to_string());
        fs::write(&path, b"tampered bytes").unwrap();

        match store.get(digest) {
            Err(ObjectStoreError::Corrupted { expected, actual }) => {
                assert_eq!(expected, digest);
                assert_ne!(actual, digest);
            }
            other => panic!("expected Corrupted, got {other:?}"),
        }
    }

    #[test]
    fn contains_reflects_write_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemObjectStore::open(dir.path()).unwrap();
        let unknown = hash_bytes(b"not written yet");
        assert!(!store.contains(unknown));
        let digest = store.put(b"now written").unwrap();
        assert!(store.contains(digest));
    }

    #[test]
    fn objects_are_sharded_by_first_two_hex_characters() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemObjectStore::open(dir.path()).unwrap();
        let digest = store.put(b"shard me").unwrap();
        let hex = digest.to_string();
        let expected_path = dir.path().join(&hex[..2]).join(&hex);
        assert!(expected_path.is_file());
    }
}
