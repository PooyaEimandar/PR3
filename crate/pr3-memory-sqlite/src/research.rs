//! Task-owned research snapshots. Deleting a task cascades to every saved source.
//! Imported documents remain the authority for local passages; no remote sync.
use crate::{SqliteMemory, error};
use pr3_memory::MemoryError;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub(crate) const MIGRATION: &str = "
CREATE TABLE research_runs(task INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
 topic TEXT NOT NULL, web INTEGER NOT NULL, search_state TEXT NOT NULL, search_error TEXT, searches INTEGER NOT NULL DEFAULT 0);
CREATE TABLE research_candidates(task INTEGER NOT NULL REFERENCES research_runs(task) ON DELETE CASCADE,
 id TEXT NOT NULL, url TEXT NOT NULL, title TEXT NOT NULL, snippet TEXT NOT NULL,
 PRIMARY KEY(task,id));
CREATE TABLE research_sources(task INTEGER NOT NULL REFERENCES research_runs(task) ON DELETE CASCADE,
 source TEXT NOT NULL, title TEXT NOT NULL, location TEXT NOT NULL, text TEXT NOT NULL,
 captured_at INTEGER NOT NULL, document INTEGER REFERENCES documents(id) ON DELETE CASCADE,
 PRIMARY KEY(task,source));
CREATE TABLE research_settings(name TEXT PRIMARY KEY, value TEXT NOT NULL);
";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchProvider {
    Tavily,
    Brave,
}
impl SearchProvider {
    pub fn name(self) -> &'static str {
        match self {
            Self::Tavily => "Tavily",
            Self::Brave => "Brave Search",
        }
    }
    fn setting(self) -> &'static str {
        match self {
            Self::Tavily => "tavily_key",
            Self::Brave => "brave_key",
        }
    }
    fn value(self) -> &'static str {
        match self {
            Self::Tavily => "tavily",
            Self::Brave => "brave",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Research {
    pub task: u64,
    pub topic: String,
    pub web: bool,
    pub search_state: String,
    pub search_error: Option<String>,
    pub searches: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub url: String,
    pub title: String,
    pub snippet: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub title: String,
    pub location: String,
    pub text: String,
    pub captured_at: u64,
    pub document: Option<i64>,
}
fn invalid(message: &str) -> MemoryError {
    MemoryError(message.into())
}

fn sql_id(value: u64) -> Result<i64, MemoryError> {
    i64::try_from(value).map_err(|_| invalid("Research identifier exceeds storage range"))
}

/// Compact exact references for model arguments. Canonical IDs remain the storage,
/// audit and UI authority. Binding the task prevents reuse in a different task;
/// resolution rejects collisions rather than matching a prefix or guessing.
pub fn source_reference(task: u64, source: &str) -> String {
    reference("s", task, source)
}
pub fn candidate_reference(task: u64, candidate: &str) -> String {
    reference("c", task, candidate)
}
fn reference(kind: &str, task: u64, id: &str) -> String {
    let hash = pr3_core::trusted::Hash::of(&[
        b"pr3-research-reference-v1",
        kind.as_bytes(),
        &task.to_le_bytes(),
        id.as_bytes(),
    ])
    .to_hex();
    format!("{kind}:{}", &hash[..16])
}
fn unique_reference<T>(mut matches: impl Iterator<Item = T>) -> Result<Option<T>, MemoryError> {
    let found = matches.next();
    if matches.next().is_some() {
        return Err(invalid(
            "Ambiguous research reference; use the complete source ID",
        ));
    }
    Ok(found)
}

impl SqliteMemory {
    pub fn research_source_reference(
        &self,
        task: u64,
        id: &str,
    ) -> Result<Option<Evidence>, MemoryError> {
        if !id.starts_with("s:") {
            return self.research_source(task, id);
        }
        let found = unique_reference(
            self.research_sources(task)?
                .into_iter()
                .filter(|s| source_reference(task, &s.source) == id),
        )?;
        match found {
            Some(s) => self.research_source(task, &s.source),
            None => Ok(None),
        }
    }

    pub fn research_candidate_reference(
        &self,
        task: u64,
        id: &str,
    ) -> Result<Option<Candidate>, MemoryError> {
        unique_reference(
            self.research_candidates(task)?
                .into_iter()
                .filter(|c| c.id == id || candidate_reference(task, &c.id) == id),
        )
    }

    /// Includes snapshots saved before a stopped turn could persist tool metadata.
    pub fn research_document_tasks(&self, document: i64) -> Result<Vec<u64>, MemoryError> {
        self.conn
            .prepare("SELECT DISTINCT task FROM research_sources WHERE document=?1")
            .map_err(error)?
            .query_map([document], |r| Ok(crate::unsigned(r.get(0)?)))
            .map_err(error)?
            .collect::<Result<_, _>>()
            .map_err(error)
    }

    pub fn begin_research(&mut self, task: u64, topic: &str, web: bool) -> Result<(), MemoryError> {
        if topic.trim().is_empty() || topic.len() > 512 || topic.chars().any(|c| c.is_control()) {
            return Err(invalid(
                "Use a research topic of 1–512 UTF-8 bytes on one line",
            ));
        }
        if let Some(previous) = self.research(task)? {
            return if previous.topic == topic && previous.web == web {
                Ok(())
            } else {
                Err(invalid(
                    "Start a new research task to change the topic or web permission",
                ))
            };
        }
        self.conn
            .execute(
                "INSERT INTO research_runs(task,topic,web,search_state,search_error) VALUES(?1,?2,?3,?4,NULL)",
                params![sql_id(task)?, topic, web, if web { "pending" } else { "disabled" }],
            )
            .map_err(error)?;
        Ok(())
    }
    pub fn research(&self, task: u64) -> Result<Option<Research>, MemoryError> {
        self.conn
            .query_row(
                "SELECT topic,web,search_state,search_error,searches FROM research_runs WHERE task=?1",
                [sql_id(task)?],
                |r| {
                    Ok(Research {
                        task,
                        topic: r.get(0)?,
                        web: r.get(1)?,
                        search_state: r.get(2)?,
                        search_error: r.get(3)?,
                        searches: r.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(error)
    }
    pub fn research_search_result(
        &mut self,
        task: u64,
        failure: Option<&str>,
    ) -> Result<(), MemoryError> {
        self.conn
            .execute(
                "UPDATE research_runs SET search_state=?2,search_error=?3 WHERE task=?1",
                params![
                    sql_id(task)?,
                    if failure.is_some() {
                        "failed"
                    } else {
                        "complete"
                    },
                    failure
                ],
            )
            .map_err(error)?;
        Ok(())
    }
    pub fn mark_research_search(&mut self, task: u64) -> Result<(), MemoryError> {
        self.conn
            .execute(
                "UPDATE research_runs SET searches=min(searches+1,1000000) WHERE task=?1",
                [sql_id(task)?],
            )
            .map_err(error)?;
        Ok(())
    }
    pub fn research_candidates(&self, task: u64) -> Result<Vec<Candidate>, MemoryError> {
        self.conn
            .prepare(
                "SELECT id,url,title,snippet FROM research_candidates WHERE task=?1 ORDER BY rowid",
            )
            .map_err(error)?
            .query_map([sql_id(task)?], |r| {
                Ok(Candidate {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    snippet: r.get(3)?,
                })
            })
            .map_err(error)?
            .collect::<Result<_, _>>()
            .map_err(error)
    }
    pub fn add_research_candidate(
        &mut self,
        task: u64,
        url: &str,
        title: &str,
        snippet: &str,
    ) -> Result<(), MemoryError> {
        if url.len() > 2048 || title.len() > 400 || snippet.len() > 800 {
            return Err(invalid("Search result exceeds research limits"));
        }
        let id = format!(
            "candidate:{task}:{}",
            &pr3_core::trusted::Hash::of(&[url.as_bytes()]).to_hex()[..32]
        );
        if self.research_candidates(task)?.iter().any(|c| c.id == id) {
            return Ok(());
        }
        if self.research_candidates(task)?.len() >= 8 {
            return Err(invalid("This research task already has eight web sources"));
        }
        self.conn
            .execute(
                "INSERT INTO research_candidates VALUES(?1,?2,?3,?4,?5)",
                params![sql_id(task)?, id, url, title, snippet],
            )
            .map_err(error)?;
        Ok(())
    }
    pub fn save_research_sources(
        &mut self,
        task: u64,
        sources: &[Evidence],
    ) -> Result<(), MemoryError> {
        let existing = self.research_sources(task)?;
        let new: Vec<_> = sources
            .iter()
            .filter(|s| !existing.iter().any(|old| old.source == s.source))
            .collect();
        let extra: usize = new.iter().map(|s| s.text.len()).sum();
        let global: i64 = self
            .conn
            .query_row(
                "SELECT coalesce(sum(length(CAST(text AS BLOB))),0) FROM research_sources",
                [],
                |r| r.get(0),
            )
            .map_err(error)?;
        if existing.len() + new.len() > 96
            || existing.iter().map(|s| s.text.len()).sum::<usize>() + extra > 128 * 1024
            || global + extra as i64 > 16 * 1024 * 1024
        {
            return Err(invalid(
                "Research storage is full. Delete an old research task first.",
            ));
        }
        let tx = self.conn.transaction().map_err(error)?;
        for source in new {
            if source.source.len() > 200
                || source.title.len() > 400
                || source.location.len() > 2200
                || source.text.len() > 3000
                || source.text.trim().is_empty()
            {
                return Err(invalid("Source exceeds research limits"));
            }
            tx.execute(
                "INSERT INTO research_sources VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    sql_id(task)?,
                    source.source,
                    source.title,
                    source.location,
                    source.text,
                    sql_id(source.captured_at)?,
                    source.document
                ],
            )
            .map_err(error)?;
        }
        tx.commit().map_err(error)
    }
    pub fn research_sources(&self, task: u64) -> Result<Vec<Evidence>, MemoryError> {
        self.conn.prepare("SELECT source,title,location,text,captured_at,document FROM research_sources WHERE task=?1 ORDER BY rowid").map_err(error)?
            .query_map([sql_id(task)?],|r| Ok(Evidence{source:r.get(0)?,title:r.get(1)?,location:r.get(2)?,text:r.get(3)?,captured_at:crate::unsigned(r.get(4)?),document:r.get(5)?})).map_err(error)?
            .collect::<Result<_,_>>().map_err(error)
    }
    pub fn research_source(
        &self,
        task: u64,
        source: &str,
    ) -> Result<Option<Evidence>, MemoryError> {
        let found = self
            .research_sources(task)?
            .into_iter()
            .find(|s| s.source == source);
        if let Some(ref evidence) = found
            && evidence.document.is_some()
            && self.document_passage(source)?.is_none()
        {
            return Ok(None);
        }
        Ok(found)
    }
    /// Preserve a pre-provider-selection Brave setup; new installations use Tavily.
    pub fn research_search_provider(&self) -> Result<SearchProvider, MemoryError> {
        let saved: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM research_settings WHERE name='provider'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(error)?;
        match saved.as_deref() {
            Some("tavily") => Ok(SearchProvider::Tavily),
            Some("brave") => Ok(SearchProvider::Brave),
            Some(_) => Err(invalid(
                "Unknown search provider. Choose one in Settings → Web.",
            )),
            None if self.research_search_configured(SearchProvider::Brave)? => {
                Ok(SearchProvider::Brave)
            }
            None => Ok(SearchProvider::Tavily),
        }
    }
    pub fn set_research_search_provider(
        &mut self,
        provider: SearchProvider,
    ) -> Result<(), MemoryError> {
        self.conn.execute(
            "INSERT INTO research_settings VALUES('provider',?1) ON CONFLICT(name) DO UPDATE SET value=excluded.value",
            [provider.value()],
        ).map_err(error)?;
        Ok(())
    }
    pub fn research_search_configured(
        &self,
        provider: SearchProvider,
    ) -> Result<bool, MemoryError> {
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM research_settings WHERE name=?1)",
                [provider.setting()],
                |r| r.get(0),
            )
            .map_err(error)
    }
    /// App-owned secret: never passed to model tools, conversation exports or sync.
    pub fn research_search_key(
        &self,
        provider: SearchProvider,
    ) -> Result<Option<String>, MemoryError> {
        self.conn
            .query_row(
                "SELECT value FROM research_settings WHERE name=?1",
                [provider.setting()],
                |r| r.get(0),
            )
            .optional()
            .map_err(error)
    }
    pub fn set_research_search_key(
        &mut self,
        provider: SearchProvider,
        key: &str,
    ) -> Result<(), MemoryError> {
        if !key.is_empty()
            && (!(16..=512).contains(&key.len()) || !key.bytes().all(|c| c.is_ascii_graphic()))
        {
            return Err(invalid("Enter a valid search API key without spaces"));
        }
        let selected = self.research_search_provider()?;
        let tx = self.conn.transaction().map_err(error)?;
        // Freeze the legacy/default selection before removing or adding a key.
        tx.execute(
            "INSERT OR IGNORE INTO research_settings VALUES('provider',?1)",
            [selected.value()],
        )
        .map_err(error)?;
        if key.is_empty() {
            tx.execute(
                "DELETE FROM research_settings WHERE name=?1",
                [provider.setting()],
            )
            .map_err(error)?;
        } else {
            tx.execute("INSERT INTO research_settings VALUES(?1,?2) ON CONFLICT(name) DO UPDATE SET value=excluded.value",[provider.setting(),key]).map_err(error)?;
        }
        tx.commit().map_err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TaskRecord, TaskState};
    use pr3_memory::Timestamp;
    fn task(db: &mut SqliteMemory, id: u64) {
        db.save_task(&TaskRecord {
            id,
            title: "Research plans".into(),
            parent: None,
            state: TaskState::Done,
            updated_at: Timestamp(1),
        })
        .unwrap();
        db.begin_research(id, "Compare plans", false).unwrap();
    }
    fn evidence() -> Evidence {
        Evidence {
            source: "research:1:source".into(),
            title: "Plan".into(),
            location: "https://example.org/".into(),
            text: "Price is 40.".into(),
            captured_at: 1,
            document: None,
        }
    }
    #[test]
    fn snapshots_and_scope_survive_restart_and_task_deletion_erases_them() {
        let dir = std::env::temp_dir().join(format!("pr3-research-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("db.sqlite");
        let mut db = SqliteMemory::open(&path).unwrap();
        task(&mut db, 1);
        task(&mut db, 2);
        db.save_research_sources(1, &[evidence()]).unwrap();
        db.add_research_candidate(1, "https://example.org/", "Plan", "")
            .unwrap();
        let source_ref = source_reference(1, "research:1:source");
        let candidate_id = db.research_candidates(1).unwrap()[0].id.clone();
        let candidate_ref = candidate_reference(1, &candidate_id);
        assert!(
            db.research_source(2, "research:1:source")
                .unwrap()
                .is_none()
        );
        // Identical text/IDs in another task still have distinct references.
        db.save_research_sources(2, &[evidence()]).unwrap();
        assert!(
            db.research_source_reference(2, &source_ref)
                .unwrap()
                .is_none()
        );
        assert!(
            db.research_candidate_reference(2, &candidate_ref)
                .unwrap()
                .is_none()
        );
        assert!(db.begin_research(1, "Changed topic", true).is_err());
        drop(db);
        let mut db = SqliteMemory::open(&path).unwrap();
        assert_eq!(db.research_sources(1).unwrap()[0].text, "Price is 40.");
        assert_eq!(
            db.research_source_reference(1, &source_ref)
                .unwrap()
                .unwrap()
                .source,
            "research:1:source"
        );
        assert_eq!(
            db.research_candidate_reference(1, &candidate_ref)
                .unwrap()
                .unwrap()
                .id,
            candidate_id
        );
        for wrong in [
            &source_ref[..source_ref.len() - 1],
            "research:1:",
            "s:0000000000000000",
        ] {
            assert!(db.research_source_reference(1, wrong).unwrap().is_none());
        }
        assert_eq!(db.research(1).unwrap().unwrap().topic, "Compare plans");
        db.delete_task(1, Timestamp(2)).unwrap();
        assert!(db.research(1).unwrap().is_none());
        assert!(db.research_sources(1).unwrap().is_empty());
        assert!(db.research_candidates(1).unwrap().is_empty());
        assert!(
            db.research_source_reference(1, &source_ref)
                .unwrap()
                .is_none()
        );
        assert!(
            db.research_candidate_reference(1, &candidate_ref)
                .unwrap()
                .is_none()
        );
        assert!(db.research(2).unwrap().is_some());
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn ambiguous_references_are_never_resolved_by_first_match() {
        assert!(unique_reference(["one", "two"].into_iter()).is_err());
        assert_eq!(unique_reference(["one"].into_iter()).unwrap(), Some("one"));
        assert_eq!(unique_reference(std::iter::empty::<&str>()).unwrap(), None);
    }
    #[test]
    fn removal_bounds_and_credentials_do_not_mix_with_evidence() {
        let mut db = SqliteMemory::open_in_memory().unwrap();
        task(&mut db, 1);
        let document = db.import_document("plan.md", "Price is 40.").unwrap();
        let passage = db.search_documents("Price").unwrap().remove(0);
        let mut source = evidence();
        source.source = passage.source;
        source.document = Some(document.id);
        db.save_research_sources(1, &[source.clone()]).unwrap();
        db.remove_document(document.id).unwrap();
        assert!(db.research_source(1, &source.source).unwrap().is_none());
        let mut oversized = evidence();
        oversized.text = "x".repeat(3001);
        assert!(
            db.save_research_sources(1, &[evidence(), oversized])
                .is_err()
        );
        assert!(
            db.research_sources(1).unwrap().is_empty(),
            "batch rolls back"
        );
        let secret = "synthetic-key-not-a-real-credential";
        for provider in [SearchProvider::Tavily, SearchProvider::Brave] {
            db.set_research_search_key(provider, secret).unwrap();
            assert_eq!(
                db.research_search_key(provider).unwrap().as_deref(),
                Some(secret)
            );
            assert!(db.set_research_search_key(provider, "bad\nheader").is_err());
            assert!(
                db.set_research_search_key(provider, &"x".repeat(513))
                    .is_err()
            );
        }
        assert!(
            !serde_json::to_string(&db.research(1).unwrap())
                .unwrap()
                .contains(secret)
        );
        db.continuity_init(&"a".repeat(64)).unwrap();
        assert_eq!(
            db.continuity_generation().unwrap(),
            0,
            "keys do not enter the sync journal"
        );
        for provider in [SearchProvider::Tavily, SearchProvider::Brave] {
            db.set_research_search_key(provider, "").unwrap();
            assert!(!db.research_search_configured(provider).unwrap());
            assert!(db.research_search_key(provider).unwrap().is_none());
        }
        assert!(db.begin_research(u64::MAX, "topic", false).is_err());
    }

    #[test]
    fn provider_selection_preserves_legacy_brave_and_isolates_keys_across_restart() {
        use SearchProvider::{Brave, Tavily};
        let path =
            std::env::temp_dir().join(format!("pr3-search-settings-{}.sqlite", std::process::id()));
        let mut db = SqliteMemory::open(&path).unwrap();
        assert_eq!(db.research_search_provider().unwrap(), Tavily);
        // The exact row written by the original Brave-only app.
        db.conn
            .execute(
                "INSERT INTO research_settings VALUES('brave_key',?1)",
                ["synthetic-legacy-brave-key"],
            )
            .unwrap();
        assert_eq!(db.research_search_provider().unwrap(), Brave);
        db.set_research_search_key(Tavily, "synthetic-tavily-key")
            .unwrap();
        assert_eq!(
            db.research_search_provider().unwrap(),
            Brave,
            "saving does not silently switch providers"
        );
        db.set_research_search_key(Brave, "").unwrap();
        assert_eq!(
            db.research_search_provider().unwrap(),
            Brave,
            "removing does not enable a different provider"
        );
        assert!(db.research_search_configured(Tavily).unwrap());
        drop(db);
        let mut db = SqliteMemory::open(&path).unwrap();
        assert_eq!(db.research_search_provider().unwrap(), Brave);
        assert!(!db.research_search_configured(Brave).unwrap());
        assert_eq!(
            db.research_search_key(Tavily).unwrap().as_deref(),
            Some("synthetic-tavily-key")
        );
        db.set_research_search_provider(Tavily).unwrap();
        drop(db);
        let db = SqliteMemory::open(&path).unwrap();
        assert_eq!(db.research_search_provider().unwrap(), Tavily);
        assert!(serde_json::from_str::<SearchProvider>("\"https://attacker.example\"").is_err());
        drop(db);
        std::fs::remove_file(path).unwrap();
    }
}
