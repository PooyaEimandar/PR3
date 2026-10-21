//! PR3 memory on SQLite (Roadmap §4.4): provenance on every row, FTS5 search ranked by
//! bm25, versioned migrations, and forgetting that erases rather than hides.
//!
//! SQLite is bundled, with FTS5, so every platform runs the same engine. Deleted pages
//! are overwritten (`secure_delete`), so forgotten text does not linger in the file.
//! Encryption at rest comes with the M3 personal-memory work.

#![forbid(unsafe_code)]

use std::path::Path;

use pr3_memory::{
    Consent, Memory, MemoryError, MemoryId, MemoryStore, NewMemory, Privacy, Provenance, Retention,
    SessionId, Source, Status, Timestamp,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

/// Schema migrations, applied in order; the database's `user_version` is how many have
/// run. Never edit a released migration: add a new one.
const MIGRATIONS: &[&str] = &[
    // v1: memories with provenance, their sources, full-text search, tombstones.
    "
    CREATE TABLE sources (
        id INTEGER PRIMARY KEY,
        kind TEXT NOT NULL CHECK (kind IN ('conversation', 'user')),
        session INTEGER,
        -- The user's words the memory came from; erased when no live memory needs them.
        excerpt TEXT
    );
    CREATE TABLE memories (
        id INTEGER PRIMARY KEY,
        text TEXT NOT NULL,
        normalized TEXT NOT NULL,
        status TEXT NOT NULL CHECK (status IN ('proposed', 'confirmed')),
        source_id INTEGER NOT NULL REFERENCES sources(id),
        principal TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        valid_at INTEGER NOT NULL,
        confidence INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 100),
        privacy TEXT NOT NULL CHECK (privacy IN ('personal', 'sensitive')),
        consent TEXT NOT NULL CHECK (consent IN ('device_only')),
        retain_until INTEGER,
        supersedes INTEGER REFERENCES memories(id),
        superseded_by INTEGER REFERENCES memories(id),
        deleted_at INTEGER
    );
    CREATE INDEX memories_normalized ON memories(normalized) WHERE deleted_at IS NULL;
    CREATE VIRTUAL TABLE memories_fts USING fts5(
        text, content = 'memories', content_rowid = 'id', tokenize = 'porter unicode61'
    );
    CREATE TRIGGER memories_fts_insert AFTER INSERT ON memories BEGIN
        INSERT INTO memories_fts(rowid, text) VALUES (new.id, new.text);
    END;
    CREATE TRIGGER memories_fts_update AFTER UPDATE OF text ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, text) VALUES ('delete', old.id, old.text);
        INSERT INTO memories_fts(rowid, text) VALUES (new.id, new.text);
    END;
    CREATE TRIGGER memories_fts_delete AFTER DELETE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, text) VALUES ('delete', old.id, old.text);
    END;
    -- What was forgotten and when, without what it said (for sync and audit).
    CREATE TABLE memory_tombstones (
        memory_id INTEGER PRIMARY KEY,
        deleted_at INTEGER NOT NULL
    );
    ",
];

const COLUMNS: &str = "m.id, m.text, m.status, m.principal, m.created_at, m.valid_at,
    m.confidence, m.privacy, m.consent, m.retain_until, m.supersedes,
    s.kind, s.session, s.excerpt";

/// Recalled: not forgotten, not corrected, not past retention (`?now` is bound).
const LIVE: &str = "m.deleted_at IS NULL AND m.superseded_by IS NULL
    AND (m.retain_until IS NULL OR m.retain_until > :now)";

/// Common words that would match almost every memory, and words about recalling
/// rather than what to recall.
const STOPWORDS: &[&str] = &[
    "a",
    "about",
    "an",
    "and",
    "anything",
    "are",
    "did",
    "do",
    "does",
    "everything",
    "for",
    "how",
    "i",
    "in",
    "is",
    "it",
    "know",
    "me",
    "my",
    "of",
    "on",
    "or",
    "remember",
    "tell",
    "that",
    "the",
    "this",
    "to",
    "was",
    "what",
    "when",
    "where",
    "which",
    "who",
    "why",
    "you",
    "your",
];

const MAX_TERMS: usize = 12;

pub struct SqliteMemory {
    conn: Connection,
}

impl SqliteMemory {
    /// Opens (creating if needed) the memory database at `path` and brings its schema
    /// up to date.
    pub fn open(path: &Path) -> Result<Self, MemoryError> {
        Self::init(Connection::open(path).map_err(error)?)
    }

    pub fn open_in_memory() -> Result<Self, MemoryError> {
        Self::init(Connection::open_in_memory().map_err(error)?)
    }

    fn init(mut conn: Connection) -> Result<Self, MemoryError> {
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(error)?;
        conn.pragma_update(None, "secure_delete", true)
            .map_err(error)?;
        migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// How many migrations the database has run.
    pub fn schema_version(&self) -> Result<usize, MemoryError> {
        schema_version(&self.conn)
    }

    fn select(
        &self,
        filter: &str,
        order: &str,
        params: &[(&str, &dyn rusqlite::ToSql)],
    ) -> Result<Vec<Memory>, MemoryError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM memories m JOIN sources s ON s.id = m.source_id
             WHERE {filter} {order}"
        );
        let mut statement = self.conn.prepare(&sql).map_err(error)?;
        let rows = statement.query_map(params, memory).map_err(error)?;
        rows.collect::<Result<_, _>>().map_err(error)
    }
}

impl MemoryStore for SqliteMemory {
    fn remember(&mut self, new: NewMemory) -> Result<MemoryId, MemoryError> {
        let text = new.text.trim();
        if text.is_empty() {
            return Err(MemoryError("nothing to remember".to_string()));
        }
        let normalized = normalize(text);
        let tx = self.conn.transaction().map_err(error)?;
        let existing: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, status FROM memories
                 WHERE normalized = ?1 AND deleted_at IS NULL AND superseded_by IS NULL",
                params![normalized],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(error)?;
        let id = match existing {
            Some((id, status)) => {
                if new.status == Status::Confirmed && status != "confirmed" {
                    tx.execute(
                        "UPDATE memories SET status = 'confirmed',
                         confidence = MAX(confidence, ?2) WHERE id = ?1",
                        params![id, new.provenance.confidence],
                    )
                    .map_err(error)?;
                }
                id
            }
            None => insert(&tx, text, &normalized, new.status, &new.provenance, None)?,
        };
        tx.commit().map_err(error)?;
        Ok(MemoryId(id))
    }

    fn search(
        &self,
        query: &str,
        limit: usize,
        now: Timestamp,
    ) -> Result<Vec<Memory>, MemoryError> {
        let terms = search_terms(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let expression = terms
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(" OR ");
        let sql = format!(
            "SELECT {COLUMNS} FROM memories_fts f
             JOIN memories m ON m.id = f.rowid JOIN sources s ON s.id = m.source_id
             WHERE memories_fts MATCH :query AND {LIVE}
             ORDER BY bm25(memories_fts), m.id DESC LIMIT :limit"
        );
        let mut statement = self.conn.prepare(&sql).map_err(error)?;
        let rows = statement
            .query_map(
                &[
                    (":query", &expression as &dyn rusqlite::ToSql),
                    (":now", &db_time(now)?),
                    (":limit", &db_count(limit)),
                ],
                memory,
            )
            .map_err(error)?;
        rows.collect::<Result<_, _>>().map_err(error)
    }

    fn recent(&self, limit: usize, now: Timestamp) -> Result<Vec<Memory>, MemoryError> {
        self.select(
            LIVE,
            "ORDER BY m.created_at DESC, m.id DESC LIMIT :limit",
            &[(":now", &db_time(now)?), (":limit", &db_count(limit))],
        )
    }

    fn get(&self, id: MemoryId) -> Result<Option<Memory>, MemoryError> {
        Ok(self
            .select("m.id = :id AND m.deleted_at IS NULL", "", &[(":id", &id.0)])?
            .into_iter()
            .next())
    }

    fn correct(
        &mut self,
        id: MemoryId,
        text: &str,
        at: Timestamp,
    ) -> Result<MemoryId, MemoryError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(MemoryError("a correction needs text".to_string()));
        }
        let old = self
            .get(id)?
            .ok_or_else(|| MemoryError(format!("no memory {id}")))?;
        let provenance = Provenance {
            principal: old.provenance.principal,
            privacy: old.provenance.privacy,
            consent: old.provenance.consent,
            ..Provenance::from_user(at)
        };
        let tx = self.conn.transaction().map_err(error)?;
        let new = insert(
            &tx,
            text,
            &normalize(text),
            Status::Confirmed,
            &provenance,
            Some(id),
        )?;
        tx.execute(
            "UPDATE memories SET superseded_by = ?1 WHERE id = ?2",
            params![new, id.0],
        )
        .map_err(error)?;
        tx.commit().map_err(error)?;
        Ok(MemoryId(new))
    }

    /// Forgets the memory and every earlier version it corrected.
    fn forget(&mut self, id: MemoryId, at: Timestamp) -> Result<bool, MemoryError> {
        let at = db_time(at)?;
        let tx = self.conn.transaction().map_err(error)?;
        let mut next = Some(id.0);
        let mut forgot = false;
        while let Some(current) = next {
            next = tx
                .query_row(
                    "SELECT supersedes FROM memories WHERE id = ?1",
                    params![current],
                    |r| r.get(0),
                )
                .optional()
                .map_err(error)?
                .flatten();
            let erased = tx
                .execute(
                    "UPDATE memories SET text = '', normalized = '', deleted_at = ?2
                     WHERE id = ?1 AND deleted_at IS NULL",
                    params![current, at],
                )
                .map_err(error)?;
            if erased == 0 {
                continue;
            }
            forgot = true;
            tx.execute(
                "UPDATE sources SET excerpt = NULL
                 WHERE id = (SELECT source_id FROM memories WHERE id = ?1)
                 AND NOT EXISTS (SELECT 1 FROM memories
                                 WHERE source_id = sources.id AND deleted_at IS NULL)",
                params![current],
            )
            .map_err(error)?;
            tx.execute(
                "INSERT OR REPLACE INTO memory_tombstones(memory_id, deleted_at)
                 VALUES (?1, ?2)",
                params![current, at],
            )
            .map_err(error)?;
        }
        tx.commit().map_err(error)?;
        Ok(forgot)
    }
}

fn migrate(conn: &mut Connection) -> Result<(), MemoryError> {
    let version = schema_version(conn)?;
    if version > MIGRATIONS.len() {
        return Err(MemoryError(format!(
            "the memory database was written by a newer PR3 (schema v{version}, this build \
             knows v{})",
            MIGRATIONS.len()
        )));
    }
    for (done, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction().map_err(error)?;
        tx.execute_batch(sql).map_err(error)?;
        tx.pragma_update(None, "user_version", db_count(done + 1))
            .map_err(error)?;
        tx.commit().map_err(error)?;
    }
    Ok(())
}

fn schema_version(conn: &Connection) -> Result<usize, MemoryError> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(error)?;
    usize::try_from(version).map_err(|_| MemoryError(format!("bad schema version {version}")))
}

fn insert(
    tx: &Transaction<'_>,
    text: &str,
    normalized: &str,
    status: Status,
    p: &Provenance,
    supersedes: Option<MemoryId>,
) -> Result<i64, MemoryError> {
    let (kind, session, excerpt) = match &p.source {
        Source::Conversation { session, excerpt } => (
            "conversation",
            Some(i64::try_from(session.0).map_err(|_| MemoryError("bad session".into()))?),
            Some(excerpt.as_str()),
        ),
        Source::User => ("user", None, None),
    };
    tx.execute(
        "INSERT INTO sources(kind, session, excerpt) VALUES (?1, ?2, ?3)",
        params![kind, session, excerpt],
    )
    .map_err(error)?;
    let source_id = tx.last_insert_rowid();
    let retain_until = match p.retention {
        Retention::Keep => None,
        Retention::Until(t) => Some(db_time(t)?),
    };
    tx.execute(
        "INSERT INTO memories(text, normalized, status, source_id, principal, created_at,
             valid_at, confidence, privacy, consent, retain_until, supersedes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            text,
            normalized,
            status_name(status),
            source_id,
            p.principal,
            db_time(p.created_at)?,
            db_time(p.valid_at)?,
            p.confidence.min(100),
            match p.privacy {
                Privacy::Personal => "personal",
                Privacy::Sensitive => "sensitive",
            },
            match p.consent {
                Consent::DeviceOnly => "device_only",
            },
            retain_until,
            supersedes.map(|s| s.0),
        ],
    )
    .map_err(error)?;
    Ok(tx.last_insert_rowid())
}

/// Reads one row selected with [`COLUMNS`].
fn memory(r: &Row<'_>) -> rusqlite::Result<Memory> {
    let source = match r.get::<_, String>(11)?.as_str() {
        "conversation" => Source::Conversation {
            session: SessionId(r.get::<_, Option<i64>>(12)?.map_or(0, unsigned)),
            excerpt: r.get::<_, Option<String>>(13)?.unwrap_or_default(),
        },
        _ => Source::User,
    };
    Ok(Memory {
        id: MemoryId(r.get(0)?),
        text: r.get(1)?,
        status: if r.get::<_, String>(2)? == "confirmed" {
            Status::Confirmed
        } else {
            Status::Proposed
        },
        provenance: Provenance {
            source,
            principal: r.get(3)?,
            created_at: Timestamp(unsigned(r.get(4)?)),
            valid_at: Timestamp(unsigned(r.get(5)?)),
            confidence: r.get(6)?,
            privacy: if r.get::<_, String>(7)? == "sensitive" {
                Privacy::Sensitive
            } else {
                Privacy::Personal
            },
            consent: Consent::DeviceOnly,
            retention: r.get::<_, Option<i64>>(9)?.map_or(Retention::Keep, |t| {
                Retention::Until(Timestamp(unsigned(t)))
            }),
        },
        supersedes: r.get::<_, Option<i64>>(10)?.map(MemoryId),
    })
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Proposed => "proposed",
        Status::Confirmed => "confirmed",
    }
}

/// The form used to spot the same memory written twice.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '!'])
        .to_lowercase()
}

/// Words worth searching for, lowercased. Only letters and digits survive, so nothing a
/// model or user types can reach FTS5's query syntax.
fn search_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for word in query.split(|c: char| !c.is_alphanumeric()) {
        let word = word.to_lowercase();
        if word.chars().count() > 1 && !STOPWORDS.contains(&word.as_str()) && !terms.contains(&word)
        {
            terms.push(word);
        }
    }
    terms.truncate(MAX_TERMS);
    terms
}

fn db_time(t: Timestamp) -> Result<i64, MemoryError> {
    i64::try_from(t.0).map_err(|_| MemoryError(format!("time {} out of range", t.0)))
}

fn db_count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn unsigned(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

fn error(e: rusqlite::Error) -> MemoryError {
    MemoryError(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Timestamp = Timestamp(1_000_000);

    fn proposed(text: &str) -> NewMemory {
        NewMemory {
            text: text.to_string(),
            status: Status::Proposed,
            provenance: Provenance::proposed(SessionId(7), format!("user said: {text}"), NOW),
        }
    }

    fn texts(memories: &[Memory]) -> Vec<&str> {
        memories.iter().map(|m| m.text.as_str()).collect()
    }

    fn store() -> SqliteMemory {
        SqliteMemory::open_in_memory().unwrap()
    }

    #[test]
    fn migrations_run_once_and_newer_schemas_are_refused() {
        let path = std::env::temp_dir().join(format!("pr3-memory-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let mut m = SqliteMemory::open(&path).unwrap();
            assert_eq!(m.schema_version().unwrap(), MIGRATIONS.len());
            m.remember(proposed("The user's name is Ada.")).unwrap();
        }
        {
            // Reopening runs nothing again and keeps the data.
            let m = SqliteMemory::open(&path).unwrap();
            assert_eq!(m.schema_version().unwrap(), MIGRATIONS.len());
            assert_eq!(m.recent(10, NOW).unwrap().len(), 1);
            m.conn.pragma_update(None, "user_version", 99).unwrap();
        }
        let refused = SqliteMemory::open(&path).err().unwrap();
        assert!(refused.0.contains("newer PR3"), "{refused}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn privacy_settings_are_on() {
        let m = store();
        let secure: i64 = m
            .conn
            .query_row("PRAGMA secure_delete", [], |r| r.get(0))
            .unwrap();
        let foreign_keys: i64 = m
            .conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!((secure, foreign_keys), (1, 1));
    }

    #[test]
    fn provenance_round_trips() {
        let mut m = store();
        let mut new = proposed("Ada's sister Mina lives in Shiraz.");
        new.provenance.principal = "Mina".to_string();
        new.provenance.privacy = Privacy::Sensitive;
        new.provenance.valid_at = Timestamp(500);
        new.provenance.retention = Retention::Until(Timestamp(9_000_000));
        let id = m.remember(new.clone()).unwrap();
        let stored = m.get(id).unwrap().unwrap();
        assert_eq!(stored.text, new.text);
        assert_eq!(stored.status, Status::Proposed);
        assert_eq!(stored.provenance, new.provenance);
        assert_eq!(stored.supersedes, None);
    }

    #[test]
    fn the_same_memory_is_stored_once_and_user_confirmation_upgrades_it() {
        let mut m = store();
        let a = m.remember(proposed("The user's name is Ada.")).unwrap();
        let b = m.remember(proposed("the user's  name is Ada")).unwrap();
        assert_eq!(a, b);
        let confirmed = NewMemory {
            status: Status::Confirmed,
            provenance: Provenance::from_user(NOW),
            ..proposed("The user's name is Ada")
        };
        assert_eq!(m.remember(confirmed).unwrap(), a);
        let stored = m.get(a).unwrap().unwrap();
        assert_eq!(stored.status, Status::Confirmed);
        assert_eq!(stored.provenance.confidence, 100);
        assert!(m.remember(proposed("   ")).is_err());
    }

    #[test]
    fn search_ranks_by_relevance_and_understands_word_forms() {
        let mut m = store();
        m.remember(proposed("The user's favourite colour is green."))
            .unwrap();
        m.remember(proposed("The user's name is Ada.")).unwrap();
        m.remember(proposed("The user works as a nurse.")).unwrap();
        assert_eq!(
            texts(&m.search("What is my name?", 5, NOW).unwrap()),
            ["The user's name is Ada."]
        );
        assert_eq!(
            texts(&m.search("colours I like", 5, NOW).unwrap()),
            ["The user's favourite colour is green."]
        );
        // Only common words, or nothing related: no matches.
        assert!(
            m.search("what do you know about me", 2, NOW)
                .unwrap()
                .is_empty()
        );
        assert!(m.search("dogs", 2, NOW).unwrap().is_empty());
    }

    #[test]
    fn query_syntax_cannot_be_injected() {
        let mut m = store();
        m.remember(proposed("The user's name is Ada.")).unwrap();
        for query in [
            "name\" OR *",
            "NEAR(name ada)",
            "name:ada",
            "'); DROP TABLE memories;--",
        ] {
            assert!(m.search(query, 5, NOW).is_ok(), "{query}");
        }
        assert_eq!(m.recent(5, NOW).unwrap().len(), 1);
        assert_eq!(search_terms("Name? NAME, name! ok-x"), ["name", "ok"]);
    }

    #[test]
    fn expired_memories_are_not_recalled() {
        let mut m = store();
        let mut short = proposed("The user is in Paris this week.");
        short.provenance.retention = Retention::Until(Timestamp(NOW.0 + 10));
        m.remember(short).unwrap();
        assert_eq!(m.search("Paris", 5, NOW).unwrap().len(), 1);
        assert!(
            m.search("Paris", 5, Timestamp(NOW.0 + 10))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_correction_supersedes_the_proposal() {
        let mut m = store();
        let old = m.remember(proposed("The user's name is Ava.")).unwrap();
        let new = m.correct(old, "The user's name is Ada.", NOW).unwrap();
        let recalled = m.search("name", 5, NOW).unwrap();
        assert_eq!(texts(&recalled), ["The user's name is Ada."]);
        let correction = recalled.first().unwrap();
        assert_eq!(correction.id, new);
        assert_eq!(correction.status, Status::Confirmed);
        assert_eq!(correction.supersedes, Some(old));
        assert_eq!(correction.provenance.source, Source::User);
        // The old version still exists for inspection, it is just not recalled.
        assert!(m.get(old).unwrap().is_some());
        assert!(m.correct(MemoryId(999), "x", NOW).is_err());
    }

    #[test]
    fn forgetting_erases_the_memory_its_versions_and_its_source_words() {
        let mut m = store();
        let old = m.remember(proposed("The user's name is Ava.")).unwrap();
        let new = m.correct(old, "The user's name is Ada.", NOW).unwrap();
        let other = m.remember(proposed("The user likes tea.")).unwrap();

        assert!(m.forget(new, Timestamp(NOW.0 + 1)).unwrap());
        assert!(m.get(new).unwrap().is_none() && m.get(old).unwrap().is_none());
        assert!(m.search("name Ada Ava", 5, NOW).unwrap().is_empty());
        let leftover: i64 = m
            .conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE text LIKE '%Ad%' OR text LIKE '%Av%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let excerpts: i64 = m
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sources WHERE excerpt LIKE '%name%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let tombstones: i64 = m
            .conn
            .query_row("SELECT COUNT(*) FROM memory_tombstones", [], |r| r.get(0))
            .unwrap();
        assert_eq!((leftover, excerpts, tombstones), (0, 0, 2));

        // Unrelated memories are untouched; forgetting twice is a no-op.
        assert_eq!(
            texts(&m.search("tea", 5, NOW).unwrap()),
            ["The user likes tea."]
        );
        assert!(m.get(other).unwrap().is_some());
        assert!(!m.forget(new, NOW).unwrap());
    }
}
