//! Encrypted memory/task storage and an atomic plaintext-to-SQLCipher migration.
//! Platform key custody belongs to the host. This module never writes a key to disk.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

use pr3_memory::MemoryError;
use rusqlite::{Connection, OpenFlags, params};
use zeroize::Zeroizing;

use crate::{SqliteMemory, error};

/// No Debug/Serialize implementation: keys must not enter logs, traces, or model data.
pub struct DatabaseKey(Zeroizing<[u8; 32]>);

impl DatabaseKey {
    pub fn generate() -> Result<Self, MemoryError> {
        let mut bytes = Zeroizing::new([0; 32]);
        getrandom::fill(bytes.as_mut()).map_err(|_| failure("No secure randomness available"))?;
        Ok(Self(bytes))
    }

    pub fn from_hex(text: &str) -> Result<Self, MemoryError> {
        if text.len() != 64 || !text.is_ascii() {
            return Err(failure("Invalid storage key"));
        }
        let mut bytes = Zeroizing::new([0; 32]);
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
                .map_err(|_| failure("Invalid storage key"))?;
        }
        Ok(Self(bytes))
    }

    pub fn hex(&self) -> Zeroizing<String> {
        use std::fmt::Write;
        let mut text = Zeroizing::new(String::with_capacity(64));
        for byte in self.0.iter() {
            let _ = write!(text, "{byte:02x}");
        }
        text
    }

    /// Explicit user-requested export only. A code unlocks a database, not a device
    /// identity, pairing, or action approval. It cannot recreate a deleted database.
    pub fn recovery_code(&self) -> Zeroizing<String> {
        let hex = self.hex();
        let mut code = Zeroizing::new(String::from("PR3"));
        for chunk in hex.as_bytes().chunks(4) {
            code.push('-');
            for byte in chunk {
                code.push(char::from(*byte));
            }
        }
        code
    }

    pub fn from_recovery_code(code: &str) -> Result<Self, MemoryError> {
        if code.len() > 128 {
            return Err(failure("Invalid recovery code"));
        }
        let compact = Zeroizing::new(
            code.chars()
                .filter(|c| !c.is_whitespace() && *c != '-')
                .collect::<String>(),
        );
        Self::from_hex(
            compact
                .strip_prefix("PR3")
                .ok_or_else(|| failure("Invalid recovery code"))?,
        )
    }

    fn sql_key(&self) -> Zeroizing<String> {
        Zeroizing::new(format!("x'{}'", self.hex().as_str()))
    }
}

fn failure(message: &str) -> MemoryError {
    MemoryError(message.to_string())
}

fn io(error: std::io::Error) -> MemoryError {
    // OS messages contain paths at most; never the key or any SQL arguments.
    failure(&error.to_string())
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn private_file(path: &Path, new: bool) -> Result<File, MemoryError> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(failure("Storage files must not be symbolic links"));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).truncate(false);
    if new {
        options.create_new(true);
    } else {
        options.create(true);
    }
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path).map_err(io)
}

/// An existing nonempty file without SQLite's plaintext header requires its old key.
/// A missing key for such a file must never trigger new-key generation.
pub fn requires_existing_key(path: &Path) -> Result<bool, MemoryError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io(e)),
    };
    // An unexpected empty file could be a damaged database, not a new installation.
    // New-installation keys are saved before the database file is first created.
    if file.metadata().map_err(io)?.len() == 0 {
        return Ok(true);
    }
    let mut header = [0; 16];
    let count = file.read(&mut header).map_err(io)?;
    Ok(count != 16 || &header != b"SQLite format 3\0")
}

fn keyed(path: &Path, key: &DatabaseKey, read_only: bool) -> Result<Connection, MemoryError> {
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let conn = Connection::open_with_flags(path, flags).map_err(error)?;
    // Ordinary SQLite silently ignores PRAGMA key. Require an actual cipher engine.
    let version: String = conn
        .query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .map_err(|_| failure("SQLCipher is unavailable"))?;
    if version.is_empty() {
        return Err(failure("SQLCipher is unavailable"));
    }
    conn.pragma_update(None, "key", key.sql_key().as_str())
        .map_err(|_| failure("Cannot configure encrypted storage"))?;
    conn.pragma_update(None, "cipher_compatibility", 4)
        .map_err(error)?;
    conn.pragma_update(None, "temp_store", "MEMORY")
        .map_err(error)?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map_err(|_| failure("Storage is locked or damaged; the existing data has been preserved"))?;
    Ok(conn)
}

fn integrity(conn: &Connection) -> Result<(), MemoryError> {
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(error)?;
    if result != "ok" {
        return Err(failure("Storage integrity check failed"));
    }
    let mut statement = conn
        .prepare("PRAGMA cipher_integrity_check")
        .map_err(error)?;
    if statement
        .query([])
        .map_err(error)?
        .next()
        .map_err(error)?
        .is_some()
    {
        return Err(failure("Encrypted storage integrity check failed"));
    }
    Ok(())
}

/// Checks a recovery code without creating, migrating, or writing the database.
pub fn verify_key(path: &Path, key: &DatabaseKey) -> Result<(), MemoryError> {
    if !requires_existing_key(path)? {
        return Err(failure("No encrypted database to recover"));
    }
    let conn = keyed(path, key, true)?;
    integrity(&conn)?;
    conn.query_row("SELECT count(*) FROM memories", [], |r| r.get::<_, i64>(0))
        .map_err(error)?;
    Ok(())
}

impl SqliteMemory {
    /// App entry point. New files are private; old plaintext files are atomically
    /// replaced only after the encrypted copy passes both database and HMAC checks.
    pub fn open_encrypted(path: &Path, key: &DatabaseKey) -> Result<Self, MemoryError> {
        let lock = private_file(&sibling(path, ".lock"), false)?;
        lock.try_lock()
            .map_err(|_| failure("Storage is already open in another PR3 process"))?;
        if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(failure("Storage files must not be symbolic links"));
        }
        let fresh = !path.try_exists().map_err(io)?;
        if fresh {
            private_file(path, true)?.sync_all().map_err(io)?;
        } else if fs::metadata(path).map_err(io)?.len() == 0 {
            return Err(failure(
                "The existing storage file is empty; it has not been reset",
            ));
        }
        if fs::metadata(path).map_err(io)?.len() > 0 && !requires_existing_key(path)? {
            migrate_plaintext(path, key)?;
        }
        let mut store = Self::init(keyed(path, key, false)?)?;
        store._storage_lock = Some(lock);
        Ok(store)
    }
}

fn migrate_plaintext(path: &Path, key: &DatabaseKey) -> Result<(), MemoryError> {
    let stage = sibling(path, ".encrypting");
    // A crash before rename leaves the original canonical. Discard only our staging
    // files under the process lock, then make a fresh snapshot of that original.
    for path in [
        &stage,
        &sibling(&stage, "-journal"),
        &sibling(&stage, "-wal"),
        &sibling(&stage, "-shm"),
    ] {
        match fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io(e)),
            _ => {}
        }
    }
    private_file(&stage, true)?;
    let source =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(error)?;
    source
        .pragma_update(None, "temp_store", "MEMORY")
        .map_err(error)?;
    source
        .pragma_update(None, "locking_mode", "EXCLUSIVE")
        .map_err(error)?;
    // Checkpoint any old WAL and remove it before swapping filenames. Fail if another
    // SQLite user is active. The exclusive connection keeps its lock through export.
    let mode: String = source
        .query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))
        .map_err(error)?;
    if mode != "delete" {
        return Err(failure(
            "Close other PR3 processes before encrypting storage",
        ));
    }
    source.execute_batch("BEGIN EXCLUSIVE").map_err(error)?;
    let version: i64 = source
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(error)?;
    source
        .execute(
            "ATTACH DATABASE ?1 AS encrypted KEY ?2",
            params![stage.to_string_lossy(), key.sql_key().as_str()],
        )
        .map_err(|_| failure("Cannot create encrypted migration copy"))?;
    source
        .pragma_update(Some("encrypted"), "cipher_compatibility", 4)
        .map_err(error)?;
    source
        .query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))
        .map_err(error)?;
    source
        .pragma_update(Some("encrypted"), "user_version", version)
        .map_err(error)?;
    source
        .execute_batch("COMMIT; DETACH DATABASE encrypted")
        .map_err(error)?;
    let copy = keyed(&stage, key, true)?;
    integrity(&copy)?;
    drop(copy);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&stage)
        .map_err(io)?
        .sync_all()
        .map_err(io)?;
    drop(source);
    fs::rename(&stage, path).map_err(io)?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        File::open(parent).map_err(io)?.sync_all().map_err(io)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Said, TaskRecord, TaskState};
    use pr3_memory::{MemoryStore, NewMemory, Provenance, Status, Timestamp};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "pr3-cipher-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn db(&self) -> PathBuf {
            self.0.join("memory.sqlite")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn seed(store: &mut SqliteMemory) {
        store
            .remember(NewMemory {
                text: "private-memory-marker-7429".into(),
                status: Status::Confirmed,
                provenance: Provenance::from_user(Timestamp(1)),
            })
            .unwrap();
        store
            .save_task(&TaskRecord {
                id: 1,
                title: "private-task-marker-9318".into(),
                parent: None,
                state: TaskState::Working,
                updated_at: Timestamp(1),
            })
            .unwrap();
        store
            .add_said(
                1,
                Timestamp(2),
                &Said::User("private-chat-marker-5126".into()),
            )
            .unwrap();
    }

    #[test]
    fn encrypted_storage_links_sqlite_with_the_wal_disclosure_fix() {
        let temp = Temp::new();
        let key = DatabaseKey::generate().unwrap();
        let store = SqliteMemory::open_encrypted(&temp.db(), &key).unwrap();
        let sqlite: String = store
            .conn
            .query_row("SELECT sqlite_version()", [], |r| r.get(0))
            .unwrap();
        // RUSTSEC-2026-0459: upgrading the plain SQLite bundle alone is not
        // enough. The SQLCipher engine actually linked by each platform must
        // include SQLite 3.54.0's mismatched-WAL page-size fix too.
        assert!(
            rusqlite::version_number() >= 3_054_000,
            "SQLCipher still embeds vulnerable SQLite {sqlite}"
        );
    }

    #[test]
    fn encrypted_round_trip_wrong_key_and_recovery_preserve_data() {
        let temp = Temp::new();
        let key = DatabaseKey::generate().unwrap();
        let code = key.recovery_code();
        let mut store = SqliteMemory::open_encrypted(&temp.db(), &key).unwrap();
        seed(&mut store);
        assert!(SqliteMemory::open_encrypted(&temp.db(), &key).is_err());
        drop(store);
        let before = fs::read(temp.db()).unwrap();
        for marker in [
            "SQLite format 3",
            "private-memory-marker",
            "private-task-marker",
            "private-chat-marker",
        ] {
            assert!(
                !before
                    .windows(marker.len())
                    .any(|bytes| bytes == marker.as_bytes())
            );
        }
        assert!(requires_existing_key(&temp.db()).unwrap());
        assert!(SqliteMemory::open(&temp.db()).is_err());
        assert!(verify_key(&temp.db(), &DatabaseKey::generate().unwrap()).is_err());
        assert!(
            SqliteMemory::open_encrypted(&temp.db(), &DatabaseKey::generate().unwrap()).is_err()
        );
        assert_eq!(fs::read(temp.db()).unwrap(), before);
        let recovered = DatabaseKey::from_recovery_code(&code).unwrap();
        verify_key(&temp.db(), &recovered).unwrap();
        let store = SqliteMemory::open_encrypted(&temp.db(), &recovered).unwrap();
        assert_eq!(store.search("7429", 5, Timestamp(3)).unwrap().len(), 1);
        assert_eq!(store.tasks().unwrap()[0].chat.len(), 1);
    }

    #[test]
    fn plaintext_migration_preserves_schema_chat_and_retries_an_interrupted_copy() {
        let temp = Temp::new();
        let mut plain = SqliteMemory::open(&temp.db()).unwrap();
        seed(&mut plain);
        let version = plain.schema_version().unwrap();
        drop(plain);
        fs::write(sibling(&temp.db(), ".encrypting"), "interrupted copy").unwrap();
        let key = DatabaseKey::generate().unwrap();
        let store = SqliteMemory::open_encrypted(&temp.db(), &key).unwrap();
        assert_eq!(store.schema_version().unwrap(), version);
        assert_eq!(store.search("7429", 5, Timestamp(3)).unwrap().len(), 1);
        assert_eq!(store.tasks().unwrap()[0].chat.len(), 1);
        assert!(!sibling(&temp.db(), ".encrypting").exists());
        assert!(!sibling(&temp.db(), "-wal").exists());
    }

    #[test]
    fn invalid_recovery_never_creates_or_migrates_a_database() {
        let temp = Temp::new();
        let key = DatabaseKey::generate().unwrap();
        assert!(verify_key(&temp.db(), &key).is_err());
        assert!(!temp.db().exists());
        drop(SqliteMemory::open(&temp.db()).unwrap());
        let before = fs::read(temp.db()).unwrap();
        assert!(verify_key(&temp.db(), &key).is_err());
        assert_eq!(fs::read(temp.db()).unwrap(), before);
        assert!(DatabaseKey::from_recovery_code("a wrong code").is_err());
    }

    #[test]
    fn damaged_ciphertext_is_rejected_without_replacing_it() {
        let temp = Temp::new();
        let key = DatabaseKey::generate().unwrap();
        let mut store = SqliteMemory::open_encrypted(&temp.db(), &key).unwrap();
        seed(&mut store);
        drop(store);
        let mut bytes = fs::read(temp.db()).unwrap();
        bytes[4200] ^= 0x80;
        fs::write(temp.db(), &bytes).unwrap();
        assert!(verify_key(&temp.db(), &key).is_err());
        assert_eq!(fs::read(temp.db()).unwrap(), bytes);
        fs::write(temp.db(), []).unwrap();
        assert!(SqliteMemory::open_encrypted(&temp.db(), &key).is_err());
        assert!(fs::read(temp.db()).unwrap().is_empty());
    }

    #[test]
    fn failed_migration_keeps_the_original_until_a_verified_retry() {
        let temp = Temp::new();
        let mut source = SqliteMemory::open(&temp.db()).unwrap();
        seed(&mut source);
        source
            .conn
            .execute_batch("BEGIN; SELECT count(*) FROM memories;")
            .unwrap();
        let before = fs::read(temp.db()).unwrap();
        let key = DatabaseKey::generate().unwrap();
        assert!(SqliteMemory::open_encrypted(&temp.db(), &key).is_err());
        assert_eq!(fs::read(temp.db()).unwrap(), before);
        assert_eq!(source.search("7429", 5, Timestamp(3)).unwrap().len(), 1);
        drop(source);
        let store = SqliteMemory::open_encrypted(&temp.db(), &key).unwrap();
        assert_eq!(store.search("7429", 5, Timestamp(3)).unwrap().len(), 1);
    }
}
