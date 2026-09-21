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
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use nexo_integrity::{hash_bytes, Sha256Digest};

/// Default per-object ceiling: `put`/`get` neither write nor read past
/// this many bytes for one object. Without a bound here, this store would
/// allocate and write an arbitrarily large blob on `put`, and later read
/// the entire file into memory on every `get` regardless of size — a real
/// gap found by the same red-team pass that found RT-001-01 (see
/// docs/RED_TEAM_ROUND_001.md, finding RT-001-05): every other boundary in
/// this codebase that touches attacker-influenceable bytes (the sandboxed
/// extractor, `nexo-sandbox`'s own result-file read) bounds them; this one
/// did not.
pub const DEFAULT_MAX_OBJECT_BYTES: u64 = 256 * 1024 * 1024;

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
    /// `put` was given more than `max_bytes`, or the object on disk (for
    /// `get`) is larger than `max_bytes`. Rejected before the write, or
    /// before more than `max_bytes + 1` bytes are read into memory.
    TooLarge,
}

impl std::fmt::Display for ObjectStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ObjectStoreError {}

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
    max_bytes: u64,
}

impl FilesystemObjectStore {
    /// Creates the root directory (and any missing parents) if it does not
    /// already exist. Objects are capped at `DEFAULT_MAX_OBJECT_BYTES`; use
    /// `open_with_limit` to set a different ceiling explicitly.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, ObjectStoreError> {
        Self::open_with_limit(root, DEFAULT_MAX_OBJECT_BYTES)
    }

    pub fn open_with_limit(
        root: impl Into<PathBuf>,
        max_bytes: u64,
    ) -> Result<Self, ObjectStoreError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root, max_bytes })
    }

    /// Writes `bytes`, addressed by their own SHA-256 digest, and returns
    /// that digest. Writing the same bytes twice — including concurrently,
    /// from separate threads — is idempotent and race-free: each call
    /// writes to its own uniquely named temporary file before renaming
    /// (atomically, on the same filesystem) into place, so two writers
    /// never share a temp path and one writer's rename can never observe
    /// the other's temp file already moved out from under it.
    ///
    /// Earlier revision used a fixed `<digest>.tmp` name shared by every
    /// caller writing the same content; a concurrent-write test
    /// (`race_probe`) demonstrated that two threads could both target that
    /// same temp path and have one `rename` fail with `NotFound` because
    /// the other had already moved it — a real, reproducible defect, not
    /// hypothetical. See docs/RED_TEAM_ROUND_001.md, finding RT-001-01.
    pub fn put(&self, bytes: &[u8]) -> Result<Sha256Digest, ObjectStoreError> {
        if bytes.len() as u64 > self.max_bytes {
            return Err(ObjectStoreError::TooLarge);
        }
        let digest = hash_bytes(bytes);
        let final_path = self.path_for(digest);
        if final_path.exists() {
            return Ok(digest);
        }
        let parent = final_path
            .parent()
            .expect("path_for always yields a file under a shard directory");
        fs::create_dir_all(parent)?;

        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(bytes)?;
        temp.as_file().sync_all()?;
        // persist() renames into place; on the rare collision where two
        // writers race to persist the same final_path, whichever loses
        // simply overwrites with byte-identical content (same digest =>
        // same bytes), which is exactly the idempotent behavior promised
        // above, not a corruption.
        temp.persist(&final_path)
            .map_err(|e| ObjectStoreError::Io(e.error))?;
        Ok(digest)
    }

    /// Reads the object addressed by `digest`, verifying on every call that
    /// the bytes on disk still hash to `digest`.
    pub fn get(&self, digest: Sha256Digest) -> Result<Vec<u8>, ObjectStoreError> {
        let path = self.path_for(digest);
        let mut file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(ObjectStoreError::NotFound(digest));
            }
            Err(err) => return Err(err.into()),
        };
        let mut bytes = Vec::new();
        io::Read::by_ref(&mut file)
            .take(self.max_bytes + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > self.max_bytes {
            return Err(ObjectStoreError::TooLarge);
        }
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

    /// Regression test for RT-001-05: `put` must reject oversized content
    /// before writing anything, and `get` must reject an oversized object
    /// on disk before reading past the limit into memory.
    #[test]
    fn put_and_get_are_bounded_by_max_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemObjectStore::open_with_limit(dir.path(), 10).unwrap();

        let result = store.put(b"this is more than ten bytes");
        assert!(matches!(result, Err(ObjectStoreError::TooLarge)));

        // Bypass `put`'s own check to simulate an object that grew past
        // the limit on disk after being written by an earlier, more
        // permissive store (or a future append), and confirm `get` still
        // catches it rather than reading it wholesale into memory.
        let oversized_digest = hash_bytes(b"this is more than ten bytes");
        let path = shard_path(dir.path(), &oversized_digest.to_string());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"this is more than ten bytes").unwrap();
        assert!(matches!(
            store.get(oversized_digest),
            Err(ObjectStoreError::TooLarge)
        ));
    }

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

    /// Regression test for RT-001-01 (docs/RED_TEAM_ROUND_001.md): before
    /// the fix, concurrent `put()` calls for identical content shared one
    /// deterministic temp-file path, and one thread's `rename` would
    /// intermittently fail with `NotFound` because another thread had
    /// already moved that same temp file out from under it.
    #[test]
    fn concurrent_put_of_identical_large_content_never_yields_corrupted_read() {
        use std::sync::Arc;
        use std::thread;

        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FilesystemObjectStore::open(dir.path()).unwrap());
        // Large enough that the write spans multiple underlying write()
        // syscalls, widening the window for an interleaved race.
        let content: Vec<u8> = (0..8_000_000u32).map(|i| (i % 251) as u8).collect();
        let content = Arc::new(content);

        for round in 0..20 {
            let mut handles = Vec::new();
            for _ in 0..8 {
                let store = Arc::clone(&store);
                let content = Arc::clone(&content);
                handles.push(thread::spawn(move || store.put(&content)));
            }
            let digests: Vec<_> = handles
                .into_iter()
                .map(|h| h.join().unwrap().unwrap())
                .collect();
            let first = digests[0];
            assert!(
                digests.iter().all(|d| *d == first),
                "round {round}: digest disagreement"
            );
            let read = store.get(first);
            assert!(read.is_ok(), "round {round}: get() failed: {:?}", read.err());
            assert_eq!(
                read.unwrap(),
                *content,
                "round {round}: content mismatch despite Ok digest"
            );
        }
    }
}
