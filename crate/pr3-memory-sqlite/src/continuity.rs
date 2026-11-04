//! Explicit per-item sharing with one paired device. Versions belong to a single
//! writer. Ownership moves durably before delivery; a lost reply never grants it back.
//! The journal contains identifiers only. Deletions redact the live outbox as well.
use crate::{SqliteMemory, StoredTask, TaskState, error};
use pr3_core::trusted::Hash;
use pr3_memory::{
    Memory, MemoryError, MemoryId, MemoryStore, Privacy, Retention, Source, Status, Timestamp,
};
use pr3_protocol::{
    ContinuityChange as Change, ContinuityPage as Page, ContinuityReply as Reply,
    ContinuityRequest as Request,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

pub(crate) const MIGRATION: &str = "
CREATE TABLE continuity_state(id INTEGER PRIMARY KEY CHECK(id=1), device TEXT NOT NULL,
 peer TEXT, enabled INTEGER NOT NULL DEFAULT 0, cursor INTEGER NOT NULL DEFAULT 0,
 acknowledged INTEGER NOT NULL DEFAULT 0, last_sync INTEGER);
CREATE TABLE continuity_records(gid TEXT PRIMARY KEY, kind TEXT NOT NULL, local_id INTEGER NOT NULL,
 owner TEXT NOT NULL, epoch INTEGER NOT NULL, revision INTEGER NOT NULL, deleted INTEGER NOT NULL,
 payload TEXT, digest TEXT NOT NULL, sequence INTEGER NOT NULL,
 UNIQUE(kind, local_id));
CREATE TABLE continuity_journal(sequence INTEGER PRIMARY KEY AUTOINCREMENT, gid TEXT NOT NULL);
-- A deletion must remove the queued text in the SAME transaction, even when offline.
CREATE TRIGGER continuity_memory_erase AFTER UPDATE OF deleted_at ON memories
 WHEN new.deleted_at IS NOT NULL BEGIN
 UPDATE continuity_records SET payload=NULL, digest='' WHERE kind='memory' AND local_id=new.id;
END;
CREATE TRIGGER continuity_task_erase AFTER DELETE ON tasks BEGIN
 UPDATE continuity_records SET payload=NULL, digest='' WHERE kind='task' AND local_id=old.id;
END;
";
const MAX_ITEM: usize = 192 * 1024;
const TOO_LARGE: &str = "This item exceeds the 192 KiB sync limit. Its latest changes remain on this device; other items can still sync.";
const MAX_PAGE: usize = 512 * 1024;
const MAX_CHANGES: usize = 64;
pub const RECOVERY_UNKNOWN: &str = "Outcome unknown: the other device may have acted since the last sync. Review its results before continuing this recovered task.";
const REVIEW: &str = "Moved from another device. Review the conversation and ask again; previous approvals do not transfer.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum Content {
    Memory(Memory),
    Task(StoredTask),
}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    pub enabled: bool,
    pub peer: Option<String>,
    pub last_sync: Option<u64>,
    pub pending: u64,
    pub generation: u64,
    pub items: Vec<Item>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub kind: String,
    pub id: i64,
    pub owned: bool,
    pub deleted: bool,
    pub problem: Option<String>,
}

fn fail(s: impl Into<String>) -> MemoryError {
    MemoryError(s.into())
}
fn key(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
fn hash(s: &str) -> String {
    Hash::of(&[s.as_bytes()]).to_hex()
}

impl SqliteMemory {
    pub fn continuity_generation(&self) -> Result<u64, MemoryError> {
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(sequence),0) FROM continuity_journal",
                [],
                |r| Ok(r.get::<_, i64>(0)? as u64),
            )
            .map_err(error)
    }
    pub fn continuity_task_watermark(&self) -> Result<u64, MemoryError> {
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(local_id),0) FROM continuity_records WHERE kind='task'",
                [],
                |r| Ok(r.get::<_, i64>(0)? as u64),
            )
            .map_err(error)
    }
    pub fn continuity_init(&mut self, device: &str) -> Result<(), MemoryError> {
        if !key(device) {
            return Err(fail("Invalid continuity identity"));
        }
        self.conn
            .execute(
                "INSERT OR IGNORE INTO continuity_state(id,device) VALUES(1,?1)",
                [device],
            )
            .map_err(error)?;
        if self.identity()? != device {
            return Err(fail(
                "Device key changed. Restore the original device key before syncing this database.",
            ));
        }
        Ok(())
    }
    fn identity(&self) -> Result<String, MemoryError> {
        self.conn
            .query_row("SELECT device FROM continuity_state WHERE id=1", [], |r| {
                r.get(0)
            })
            .map_err(error)
    }
    pub fn continuity_info(&self) -> Result<Info, MemoryError> {
        let mut info = self.conn.query_row("SELECT enabled,peer,last_sync,(SELECT COUNT(*) FROM continuity_records WHERE sequence>acknowledged) FROM continuity_state WHERE id=1", [], |r| Ok(Info{enabled:r.get(0)?,peer:r.get(1)?,last_sync:r.get::<_,Option<i64>>(2)?.map(|v|v as u64),pending:r.get::<_,i64>(3)? as u64,generation:0,items:vec![]})).map_err(error)?;
        info.generation = self.continuity_generation()?;
        let me = self.identity()?;
        let mut stmt = self
            .conn
            .prepare("SELECT kind,local_id,owner,deleted FROM continuity_records")
            .map_err(error)?;
        info.items = stmt
            .query_map([], |r| {
                Ok(Item {
                    kind: r.get(0)?,
                    id: r.get(1)?,
                    owned: r.get::<_, String>(2)? == me,
                    deleted: r.get(3)?,
                    problem: None,
                })
            })
            .map_err(error)?
            .collect::<Result<_, _>>()
            .map_err(error)?;
        for item in &mut info.items {
            if item.owned
                && !item.deleted
                && self
                    .content(&item.kind, item.id, Timestamp(0))?
                    .is_some_and(|p| p.len() > MAX_ITEM)
            {
                item.problem = Some(TOO_LARGE.into());
            }
        }
        Ok(info)
    }
    /// Reconcile the continuity scope with the host's authenticated pairing records.
    /// Pairing enables it; unpairing disables it without discarding replicas or tombstones.
    /// An existing scope cannot be reassigned to a different peer's identity.
    pub fn continuity_pairing(&mut self, peers: &[String]) -> Result<(), MemoryError> {
        let current = self.continuity_info()?.peer;
        let paired = current
            .as_ref()
            .filter(|key| peers.contains(key))
            .or_else(|| peers.first());
        if let Some(old) = &current
            && !peers.contains(old)
        {
            self.continuity_enable(old, false)?;
        }
        if let Some(key) = paired {
            self.continuity_enable(key, true)?;
        }
        Ok(())
    }

    /// Internal scope state, maintained by the host's pairing lifecycle.
    pub fn continuity_enable(&mut self, peer: &str, enabled: bool) -> Result<(), MemoryError> {
        if !key(peer) || peer == self.identity()? {
            return Err(fail("Invalid continuity peer"));
        }
        let old = self.continuity_info()?.peer;
        if old.as_deref().is_some_and(|p| p != peer) {
            return Err(fail(
                "Continuity is bound to another device. Recover needed copies and reset continuity before choosing a different peer.",
            ));
        }
        self.conn
            .execute(
                "UPDATE continuity_state SET peer=?1,enabled=?2 WHERE id=1
                 AND (peer IS NOT ?1 OR enabled != ?2)",
                params![peer, enabled],
            )
            .map_err(error)?;
        Ok(())
    }
    fn permitted(&self, peer: &str) -> Result<(), MemoryError> {
        let info = self.continuity_info()?;
        if !info.enabled || info.peer.as_deref() != Some(peer) {
            return Err(fail(
                "Pair these devices to sync their selected memories and tasks.",
            ));
        }
        Ok(())
    }
    /// Checked by storage mutations, as well as the app's task controls.
    pub fn continuity_writable(&self, kind: &str, id: i64) -> Result<(), MemoryError> {
        let owner: Option<(String, bool)> = self
            .conn
            .query_row(
                "SELECT owner,deleted FROM continuity_records WHERE kind=?1 AND local_id=?2",
                params![kind, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(error)?;
        if let Some((owner, deleted)) = owner {
            if deleted {
                return Err(fail("This shared item was deleted."));
            }
            if owner != self.identity()? {
                return Err(fail(
                    "This item is owned by your other device. Move it back from there, or recover a separate copy.",
                ));
            }
        }
        Ok(())
    }
    fn content(&self, kind: &str, id: i64, now: Timestamp) -> Result<Option<String>, MemoryError> {
        let content = match kind {
            "memory" => {
                let Some(mut m) = self.get(MemoryId(id))? else {
                    return Ok(None);
                };
                if m.status != Status::Confirmed
                    || m.provenance.privacy != Privacy::Personal
                    || matches!(m.provenance.retention,Retention::Until(t) if t<=now)
                {
                    return Ok(None);
                }
                // Share the chosen fact, not the surrounding conversation.
                if let Source::Conversation { excerpt, session } = &mut m.provenance.source {
                    excerpt.clear();
                    session.0 = 0;
                }
                m.id = MemoryId(0);
                m.supersedes = None;
                Content::Memory(m)
            }
            "task" => {
                let Some(mut task) = self.tasks()?.into_iter().find(|t| t.record.id == id as u64)
                else {
                    return Ok(None);
                };
                task.record.id = 0;
                task.record.parent = None;
                task.summary = None;
                Content::Task(task)
            }
            _ => return Err(fail("Unknown continuity item")),
        };
        let text = serde_json::to_string(&content).map_err(|e| fail(e.to_string()))?;
        Ok(Some(text))
    }
    /// User-selected records only; new memories/tasks remain local by default.
    pub fn continuity_share(
        &mut self,
        kind: &str,
        id: i64,
        move_ownership: bool,
        now: Timestamp,
    ) -> Result<(), MemoryError> {
        let info = self.continuity_info()?;
        let peer = info
            .peer
            .ok_or_else(|| fail("Choose a paired device first"))?;
        self.permitted(&peer)?;
        self.continuity_writable(kind, id)?;
        let payload = self.content(kind, id, now)?.ok_or_else(|| {
            fail("Only an existing task or confirmed, non-sensitive memory can be shared.")
        })?;
        if payload.len() > MAX_ITEM {
            return Err(fail(TOO_LARGE));
        }
        if kind == "task" {
            let c: Content = serde_json::from_str(&payload).map_err(|e| fail(e.to_string()))?;
            if matches!(
                c,
                Content::Task(StoredTask {
                    record: crate::TaskRecord {
                        state: TaskState::Working,
                        ..
                    },
                    ..
                })
            ) {
                return Err(fail("Stop the task before moving or sharing it."));
            }
        }
        let me = self.identity()?;
        let tx = self.conn.transaction().map_err(error)?;
        let existing: Option<(String, u64, u64)> = tx
            .query_row(
                "SELECT gid,epoch,revision FROM continuity_records WHERE kind=?1 AND local_id=?2",
                params![kind, id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get::<_, i64>(1)? as u64,
                        r.get::<_, i64>(2)? as u64,
                    ))
                },
            )
            .optional()
            .map_err(error)?;
        let (gid, epoch, revision) = match existing {
            Some(v) => v,
            None => {
                let random: String = tx
                    .query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))
                    .map_err(error)?;
                (format!("{me}:{random}"), 0, 0)
            }
        };
        let change = Change {
            sequence: 0,
            id: gid,
            kind: kind.into(),
            owner: if move_ownership { peer } else { me },
            epoch: epoch + u64::from(move_ownership),
            revision: if move_ownership { 1 } else { revision + 1 },
            payload: Some(payload),
        };
        put(&tx, &change, id)?;
        tx.commit().map_err(error)
    }
    /// Refresh the outbox from durable data; survives a crash before journaling.
    fn capture(&mut self, now: Timestamp) -> Result<(), MemoryError> {
        let me = self.identity()?;
        let records = self.records()?;
        for (mut c, id, digest, deleted) in records {
            if deleted {
                continue;
            }
            // A local deletion is allowed on either device and wins over every version.
            let exists = match c.kind.as_str() {
                "memory" => self.get(MemoryId(id))?.is_some_and(
                    |m| !matches!(m.provenance.retention,Retention::Until(t) if t<=now),
                ),
                "task" => self
                    .conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
                        [id],
                        |r| r.get::<_, bool>(0),
                    )
                    .map_err(error)?,
                _ => false,
            };
            let payload = if !exists {
                None
            } else if c.owner == me {
                self.content(&c.kind, id, now)?
            } else {
                continue;
            };
            // A growing chat must not starve unrelated updates or tombstones.
            // Keep its previous replica and ownership unchanged; Info exposes the
            // per-item problem, and an explicit move still refuses this payload.
            if payload.as_ref().is_some_and(|p| p.len() > MAX_ITEM) {
                continue;
            }
            let next_digest = payload.as_ref().map(|s| hash(s)).unwrap_or_default();
            if c.payload.is_none() || digest != next_digest {
                c.payload = payload;
                c.revision += 1;
                let tx = self.conn.transaction().map_err(error)?;
                put(&tx, &c, id)?;
                tx.commit().map_err(error)?;
            }
        }
        Ok(())
    }
    fn records(&self) -> Result<Vec<(Change, i64, String, bool)>, MemoryError> {
        let mut stmt=self.conn.prepare("SELECT sequence,gid,kind,owner,epoch,revision,payload,local_id,digest,deleted FROM continuity_records ORDER BY sequence").map_err(error)?;
        stmt.query_map([], |r| {
            Ok((
                Change {
                    sequence: r.get::<_, i64>(0)? as u64,
                    id: r.get(1)?,
                    kind: r.get(2)?,
                    owner: r.get(3)?,
                    epoch: r.get::<_, i64>(4)? as u64,
                    revision: r.get::<_, i64>(5)? as u64,
                    payload: r.get(6)?,
                },
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        })
        .map_err(error)?
        .collect::<Result<_, _>>()
        .map_err(error)
    }
    fn page(&self, after: u64) -> Result<Page, MemoryError> {
        let mut page = Page::default();
        let mut bytes = 0;
        for (c, _, _, _) in self.records()? {
            if c.sequence <= after {
                continue;
            }
            let n = c.payload.as_ref().map_or(0, String::len) + 512;
            if bytes + n > MAX_PAGE || page.changes.len() == MAX_CHANGES {
                break;
            }
            bytes += n;
            page.changes.push(c);
        }
        Ok(page)
    }
    pub fn continuity_request(
        &mut self,
        peer: &str,
        now: Timestamp,
    ) -> Result<Request, MemoryError> {
        self.permitted(peer)?;
        self.capture(now)?;
        let (cursor, ack): (u64, u64) = self
            .conn
            .query_row(
                "SELECT cursor,acknowledged FROM continuity_state WHERE id=1",
                [],
                |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
            )
            .map_err(error)?;
        Ok(Request {
            version: 1,
            cursor,
            page: self.page(ack)?,
        })
    }
    pub fn continuity_exchange(
        &mut self,
        peer: &str,
        request: Request,
        now: Timestamp,
    ) -> Result<Reply, MemoryError> {
        self.permitted(peer)?;
        if request.version != 1 {
            return Err(fail("Different continuity version; update both devices."));
        }
        self.capture(now)?;
        self.apply(peer, &request.page, Some(request.cursor), now)?;
        let cursor = self
            .conn
            .query_row("SELECT cursor FROM continuity_state WHERE id=1", [], |r| {
                Ok(r.get::<_, i64>(0)? as u64)
            })
            .map_err(error)?;
        Ok(Reply {
            acknowledged: cursor,
            page: self.page(request.cursor)?,
        })
    }
    pub fn continuity_accept(
        &mut self,
        peer: &str,
        reply: Reply,
        now: Timestamp,
    ) -> Result<(), MemoryError> {
        self.permitted(peer)?;
        self.capture(now)?;
        self.apply(peer, &reply.page, Some(reply.acknowledged), now)
    }
    fn apply(
        &mut self,
        peer: &str,
        page: &Page,
        ack: Option<u64>,
        now: Timestamp,
    ) -> Result<(), MemoryError> {
        if page.changes.len() > MAX_CHANGES
            || page
                .changes
                .iter()
                .map(|c| {
                    c.payload
                        .as_ref()
                        .map_or(0, String::len)
                        .saturating_add(512)
                })
                .sum::<usize>()
                > MAX_PAGE
        {
            return Err(fail("Continuity page is too large"));
        }
        let me = self.identity()?;
        let tx = self.conn.transaction().map_err(error)?;
        let mut cursor: u64 = tx
            .query_row("SELECT cursor FROM continuity_state WHERE id=1", [], |r| {
                Ok(r.get::<_, i64>(0)? as u64)
            })
            .map_err(error)?;
        if let Some(ack) = ack {
            let max: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(sequence),0) FROM continuity_journal",
                    [],
                    |r| r.get(0),
                )
                .map_err(error)?;
            if ack > max as u64 {
                return Err(fail(
                    "Continuity history differs. Restore the matching database before retrying.",
                ));
            }
        }
        let mut previous = 0;
        for c in &page.changes {
            if c.sequence <= previous || c.sequence > i64::MAX as u64 {
                return Err(fail("Reordered continuity page"));
            }
            previous = c.sequence;
            validate(c, &me, peer)?;
            if c.sequence <= cursor {
                continue;
            }
            let old:Option<(i64,String,u64,u64,bool,String)>=tx.query_row("SELECT local_id,owner,epoch,revision,deleted,kind FROM continuity_records WHERE gid=?1",[&c.id],|r|Ok((r.get(0)?,r.get(1)?,r.get::<_,i64>(2)? as u64,r.get::<_,i64>(3)? as u64,r.get(4)?,r.get(5)?))).optional().map_err(error)?;
            let mut install = true;
            let local = if let Some((id, owner, epoch, revision, deleted, kind)) = old {
                if c.kind != kind {
                    return Err(fail("Continuity item kind changed"));
                }
                if deleted {
                    install = false
                } else if c.payload.is_some() {
                    if c.epoch == epoch && c.revision == revision {
                        let saved: Option<String> = tx
                            .query_row(
                                "SELECT payload FROM continuity_records WHERE gid=?1",
                                [&c.id],
                                |r| r.get(0),
                            )
                            .map_err(error)?;
                        if c.owner != owner || c.payload != saved {
                            return Err(fail("Conflicting record version; no changes applied."));
                        }
                    }
                    if c.epoch < epoch || (c.epoch == epoch && c.revision <= revision) {
                        install = false
                    } else if owner != peer
                        || !((c.epoch == epoch && c.owner == owner)
                            || (c.epoch == epoch + 1 && c.owner == me && c.revision == 1))
                    {
                        return Err(fail("Conflicting ownership; no changes applied."));
                    }
                }
                id
            } else {
                if c.payload.is_some() && c.owner == me && c.epoch == 0 {
                    return Err(fail("Invalid initial owner"));
                }
                // Stable global ids map to independent local ids; collisions never
                // overwrite a task or memory already on the receiving device.
                let table = if c.kind == "memory" {
                    "memories"
                } else {
                    "tasks"
                };
                // Task ids must remain nonnegative; reserve a distant safe JS-integer range.
                if c.kind == "task" {
                    tx.query_row("SELECT MAX(4000000000000000, COALESCE((SELECT MAX(local_id)+1 FROM continuity_records WHERE kind='task'),0), COALESCE((SELECT MAX(id)+1 FROM tasks),0))",[],|r|r.get(0)).map_err(error)?
                } else {
                    tx.query_row(&format!("SELECT MAX(1,COALESCE((SELECT MAX(id)+1 FROM {table}),1),COALESCE((SELECT MAX(local_id)+1 FROM continuity_records WHERE kind='memory'),1))"),[],|r|r.get(0)).map_err(error)?
                }
            };
            if install {
                install_content(&tx, c, local, now)?;
                put(&tx, c, local)?;
            }
            cursor = c.sequence;
        }
        if let Some(ack) = ack {
            let max: u64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(sequence),0) FROM continuity_journal",
                    [],
                    |r| Ok(r.get::<_, i64>(0)? as u64),
                )
                .map_err(error)?;
            if ack > max {
                return Err(fail("Invalid continuity acknowledgement"));
            }
            tx.execute(
                "UPDATE continuity_state SET acknowledged=MAX(acknowledged,?1) WHERE id=1",
                [ack as i64],
            )
            .map_err(error)?;
        }
        tx.execute(
            "UPDATE continuity_state SET cursor=?1,last_sync=?2 WHERE id=1",
            params![cursor as i64, crate::db_time(now)?],
        )
        .map_err(error)?;
        tx.commit().map_err(error)
    }
}
fn validate(c: &Change, me: &str, peer: &str) -> Result<(), MemoryError> {
    if c.id.len() != 97
        || !c.id.starts_with(&format!("{me}:")) && !c.id.starts_with(&format!("{peer}:"))
        || !c.id.as_bytes()[65..]
            .iter()
            .copied()
            .all(|b| b.is_ascii_hexdigit())
        || ![me, peer].contains(&c.owner.as_str())
        || !["task", "memory"].contains(&c.kind.as_str())
        || c.epoch > i64::MAX as u64 - 1
        || c.revision == 0
        || c.revision > i64::MAX as u64 - 1
    {
        return Err(fail("Invalid continuity record"));
    }
    if let Some(payload) = &c.payload {
        if payload.len() > MAX_ITEM {
            return Err(fail("Continuity item too large"));
        }
        let content: Content = serde_json::from_str(payload).map_err(|e| fail(e.to_string()))?;
        match content {
            Content::Memory(m)
                if c.kind == "memory"
                    && m.status == Status::Confirmed
                    && m.provenance.privacy == Privacy::Personal
                    && !m.text.trim().is_empty() => {}
            Content::Task(t) if c.kind == "task" && t.chat.len() <= 4096 => {}
            _ => return Err(fail("Invalid shared content")),
        }
    }
    Ok(())
}
fn put(tx: &Transaction<'_>, c: &Change, id: i64) -> Result<(), MemoryError> {
    tx.execute("INSERT INTO continuity_journal(gid) VALUES(?1)", [&c.id])
        .map_err(error)?;
    let seq = tx.last_insert_rowid();
    tx.execute("INSERT INTO continuity_records(gid,kind,local_id,owner,epoch,revision,deleted,payload,digest,sequence) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(gid) DO UPDATE SET local_id=excluded.local_id,owner=excluded.owner,epoch=excluded.epoch,revision=excluded.revision,deleted=excluded.deleted,payload=excluded.payload,digest=excluded.digest,sequence=excluded.sequence",params![c.id,c.kind,id,c.owner,c.epoch as i64,c.revision as i64,c.payload.is_none(),c.payload,c.payload.as_ref().map(|s|hash(s)).unwrap_or_default(),seq]).map_err(error)?;
    Ok(())
}
fn install_content(
    tx: &Transaction<'_>,
    c: &Change,
    id: i64,
    now: Timestamp,
) -> Result<(), MemoryError> {
    if c.kind == "memory" {
        // Erase obsolete versions and their source text, including correction ancestors.
        tx.execute("WITH RECURSIVE chain(id) AS (SELECT ?1 UNION SELECT m.id FROM memories m JOIN chain ON m.superseded_by=chain.id) UPDATE memories SET text='',normalized='',principal='',deleted_at=?2 WHERE id IN chain",params![id,crate::db_time(now)?]).map_err(error)?;
        tx.execute("UPDATE sources SET excerpt=NULL WHERE NOT EXISTS(SELECT 1 FROM memories WHERE source_id=sources.id AND deleted_at IS NULL)",[]).map_err(error)?;
        tx.execute("UPDATE tasks SET summary=NULL,summarized=0", [])
            .map_err(error)?;
    } else {
        crate::task::remove(tx, id, crate::db_time(now)?, "deleted")?;
    }
    let Some(payload) = &c.payload else {
        return Ok(());
    };
    let content: Content = serde_json::from_str(payload).map_err(|e| fail(e.to_string()))?;
    match content {
        Content::Memory(mut m) => {
            if matches!(m.provenance.retention,Retention::Until(t) if t<=now) {
                return Ok(());
            }
            if let Source::Conversation { excerpt, session } = &mut m.provenance.source {
                excerpt.clear();
                session.0 = 0;
            }
            // Replace the mapped row in place; historical ids on this device stay valid.
            tx.execute("INSERT INTO sources(kind) VALUES('user')", [])
                .map_err(error)?;
            let source = tx.last_insert_rowid();
            let until = match m.provenance.retention {
                Retention::Keep => None,
                Retention::Until(t) => Some(crate::db_time(t)?),
            };
            tx.execute("INSERT INTO memories(id,text,normalized,status,source_id,principal,created_at,valid_at,confidence,privacy,consent,retain_until) VALUES(?1,?2,?3,'confirmed',?4,?5,?6,?7,?8,'personal','device_only',?9) ON CONFLICT(id) DO UPDATE SET text=excluded.text,normalized=excluded.normalized,status='confirmed',source_id=excluded.source_id,principal=excluded.principal,created_at=excluded.created_at,valid_at=excluded.valid_at,confidence=excluded.confidence,retain_until=excluded.retain_until,superseded_by=NULL,deleted_at=NULL",params![id,m.text,crate::normalize(&m.text),source,m.provenance.principal,crate::db_time(m.provenance.created_at)?,crate::db_time(m.provenance.valid_at)?,m.provenance.confidence.min(100),until]).map_err(error)?;
        }
        Content::Task(mut t) => {
            t.record.id = id as u64;
            t.record.parent = None;
            if matches!(t.record.state, TaskState::Working | TaskState::Waiting(_)) {
                t.record.state = TaskState::Failed(REVIEW.into());
            }
            crate::task::upsert(tx, &t.record)?;
            for (at, said) in t.chat {
                let (kind, text, tool, args, success) = match said {
                    crate::Said::User(v) => ("user", v, None, None, None),
                    crate::Said::Reply(v) => ("reply", v, None, None, None),
                    crate::Said::Failed(v) => ("failed", v, None, None, None),
                    crate::Said::Tool {
                        name,
                        arguments,
                        success,
                        output,
                    } => ("tool", output, Some(name), Some(arguments), Some(success)),
                };
                tx.execute("INSERT INTO messages(task_id,at,kind,text,tool,arguments,success) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,crate::db_time(at)?,kind,text,tool,args,success]).map_err(error)?;
            }
        }
    }
    Ok(())
}

// Transactions use the same ownership check without releasing the database lock.
pub(crate) fn writable_tx(tx: &Transaction<'_>, kind: &str, id: i64) -> Result<(), MemoryError> {
    let blocked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM continuity_records r,continuity_state s WHERE r.kind=?1 AND r.local_id=?2 AND (r.deleted=1 OR r.owner!=s.device))",params![kind,id],|r|r.get(0)).map_err(error)?;
    if blocked {
        Err(fail("This item is owned by the other device."))
    } else {
        Ok(())
    }
}

impl SqliteMemory {
    /// Recovery is an explicit fork, never a silent ownership steal. The old shared
    /// identity is tombstoned, so reconnecting cannot resurrect it or its approvals.
    pub fn continuity_recover(
        &mut self,
        kind: &str,
        id: i64,
        now: Timestamp,
    ) -> Result<i64, MemoryError> {
        if self.continuity_info()?.enabled {
            return Err(fail(
                "Unpair the other device before recovering a separate copy.",
            ));
        }
        let payload = self
            .content(kind, id, now)?
            .ok_or_else(|| fail("There is no live copy to recover"))?;
        let mut content: Content =
            serde_json::from_str(&payload).map_err(|e| fail(e.to_string()))?;
        if let Content::Task(t) = &mut content {
            // The retained replica cannot attest to effects after its last exchange.
            // Even an apparently retryable failure may now have executed remotely.
            t.record.state = TaskState::Failed(RECOVERY_UNKNOWN.into());
            t.chat
                .push((now, crate::Said::Failed(RECOVERY_UNKNOWN.into())));
        }
        let payload = serde_json::to_string(&content).map_err(|e| fail(e.to_string()))?;
        if payload.len() > MAX_ITEM {
            return Err(fail("Recovered task is too large to share"));
        }
        let me = self.identity()?;
        let original = self
            .records()?
            .into_iter()
            .find(|(c, n, _, deleted)| c.kind == kind && *n == id && !*deleted)
            .ok_or_else(|| fail("This item is not shared"))?
            .0;
        if original.owner == me {
            return Err(fail("You already own this item"));
        }
        let tx = self.conn.transaction().map_err(error)?;
        let next: i64 = if kind == "task" {
            tx.query_row("SELECT MAX(4000000000000000,COALESCE((SELECT MAX(id)+1 FROM tasks),0),COALESCE((SELECT MAX(local_id)+1 FROM continuity_records WHERE kind='task'),0))",[],|r|r.get(0)).map_err(error)?
        } else {
            tx.query_row("SELECT MAX(COALESCE((SELECT MAX(id)+1 FROM memories),1),COALESCE((SELECT MAX(local_id)+1 FROM continuity_records WHERE kind='memory'),1))",[],|r|r.get(0)).map_err(error)?
        };
        let random: String = tx
            .query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))
            .map_err(error)?;
        let new = Change {
            sequence: 0,
            id: format!("{me}:{random}"),
            kind: kind.into(),
            owner: me,
            epoch: 0,
            revision: 1,
            payload: Some(payload),
        };
        install_content(&tx, &new, next, now)?;
        // The recovered copy starts private; sharing it requires another explicit choice.
        let dead = Change {
            payload: None,
            revision: original.revision + 1,
            ..original
        };
        install_content(&tx, &dead, id, now)?;
        put(&tx, &dead, id)?;
        tx.commit().map_err(error)?;
        Ok(next)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pr3_memory::{NewMemory, Provenance};
    fn identities() -> (String, String) {
        ("a".repeat(64), "b".repeat(64))
    }
    fn pair() -> (SqliteMemory, SqliteMemory) {
        let (a, b) = identities();
        let mut p = SqliteMemory::open_in_memory().unwrap();
        let mut n = SqliteMemory::open_in_memory().unwrap();
        p.continuity_init(&a).unwrap();
        n.continuity_init(&b).unwrap();
        p.continuity_pairing(&[b]).unwrap();
        n.continuity_pairing(&[a]).unwrap();
        (p, n)
    }
    fn exchange(p: &mut SqliteMemory, n: &mut SqliteMemory) {
        let (a, b) = (p.identity().unwrap(), n.identity().unwrap());
        for _ in 0..4 {
            let r = p.continuity_request(&b, Timestamp(100)).unwrap();
            let reply = n.continuity_exchange(&a, r, Timestamp(100)).unwrap();
            p.continuity_accept(&b, reply, Timestamp(100)).unwrap();
        }
    }
    fn remember(p: &mut SqliteMemory, text: &str) -> MemoryId {
        p.remember(NewMemory {
            text: text.into(),
            status: Status::Confirmed,
            provenance: Provenance::from_user(Timestamp(1)),
        })
        .unwrap()
    }
    #[test]
    fn nothing_crosses_without_pairing_and_item_selection() {
        let (mut p, mut n) = pair();
        let id = remember(&mut p, "Dentist at 14:00");
        exchange(&mut p, &mut n);
        assert!(n.recent(10, Timestamp(100)).unwrap().is_empty());
        p.continuity_share("memory", id.0, false, Timestamp(10))
            .unwrap();
        n.continuity_pairing(&[]).unwrap();
        assert!(
            n.continuity_exchange(
                &identities().0,
                p.continuity_request(&identities().1, Timestamp(100))
                    .unwrap(),
                Timestamp(100)
            )
            .is_err()
        );
        assert!(n.recent(10, Timestamp(100)).unwrap().is_empty());
        n.continuity_pairing(&[identities().0]).unwrap();
        exchange(&mut p, &mut n);
        assert_eq!(
            n.recent(10, Timestamp(100)).unwrap()[0].text,
            "Dentist at 14:00"
        );
    }
    #[test]
    fn unpair_blocks_sync_and_allows_recovery_then_repair_delivers_tombstones() {
        let (mut p, mut n) = pair();
        let (a, b) = identities();
        let id = remember(&mut p, "Dentist at 14:00");
        p.continuity_share("memory", id.0, true, Timestamp(10))
            .unwrap();
        exchange(&mut p, &mut n);
        let request = p.continuity_request(&b, Timestamp(11)).unwrap();
        let delayed = n.continuity_exchange(&a, request, Timestamp(11)).unwrap();
        assert!(p.continuity_recover("memory", id.0, Timestamp(12)).is_err());

        p.continuity_pairing(&[]).unwrap();
        assert!(!p.continuity_info().unwrap().enabled);
        assert!(p.continuity_request(&b, Timestamp(12)).is_err());
        assert!(p.continuity_accept(&b, delayed, Timestamp(12)).is_err());
        assert!(
            p.continuity_share("memory", id.0, false, Timestamp(12))
                .is_err()
        );
        assert_eq!(p.get(id).unwrap().unwrap().text, "Dentist at 14:00");
        let recovered = p.continuity_recover("memory", id.0, Timestamp(13)).unwrap();
        assert_ne!(recovered, id.0);
        assert!(p.continuity_writable("memory", recovered).is_ok());

        p.continuity_pairing(&[b]).unwrap();
        assert!(p.continuity_info().unwrap().enabled);
        exchange(&mut p, &mut n);
        assert!(n.recent(10, Timestamp(100)).unwrap().is_empty());
        assert_eq!(p.recent(10, Timestamp(100)).unwrap()[0].id.0, recovered);
        assert_eq!(p.continuity_info().unwrap().pending, 0);
    }
    #[test]
    fn restored_pairings_override_old_preferences_but_never_redirect_shared_data() {
        let (a, b) = identities();
        let path = std::env::temp_dir().join(format!("pr3-pairing-{}.sqlite", std::process::id()));
        let mut p = SqliteMemory::open(&path).unwrap();
        p.continuity_init(&a).unwrap();
        p.continuity_pairing(&[]).unwrap();
        assert!(!p.continuity_info().unwrap().enabled);
        assert!(p.continuity_info().unwrap().peer.is_none());
        // Simulate an installation that had paired but left the old switch off.
        p.continuity_enable(&b, false).unwrap();
        drop(p);
        let mut p = SqliteMemory::open(&path).unwrap();
        p.continuity_init(&a).unwrap();
        p.continuity_pairing(std::slice::from_ref(&b)).unwrap();
        assert!(p.continuity_info().unwrap().enabled);
        let id = remember(&mut p, "Keep this with the original peer");
        p.continuity_share("memory", id.0, false, Timestamp(10))
            .unwrap();
        // A crash after removing pairing but before updating SQLite is reconciled.
        drop(p);
        let mut p = SqliteMemory::open(&path).unwrap();
        p.continuity_pairing(&[]).unwrap();
        assert!(!p.continuity_info().unwrap().enabled);
        assert!(p.continuity_pairing(&["c".repeat(64)]).is_err());
        let info = p.continuity_info().unwrap();
        assert!(!info.enabled);
        assert_eq!(info.peer.as_ref(), Some(&b));
        assert_eq!(info.items.len(), 1);
        p.continuity_pairing(std::slice::from_ref(&b)).unwrap();
        // Another paired phone never changes an established continuity scope.
        p.continuity_pairing(&["c".repeat(64), b.clone()]).unwrap();
        assert!(p.continuity_info().unwrap().enabled);
        assert_eq!(
            p.continuity_request(&b, Timestamp(11))
                .unwrap()
                .page
                .changes
                .len(),
            1
        );
        drop(p);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn growing_shared_chats_do_not_block_updates_or_deletions_in_either_direction() {
        let (mut p, mut n) = pair();
        let id = remember(&mut p, "Forget this shared fact");
        p.save_task(&crate::TaskRecord {
            id: 100,
            title: "Growing task".into(),
            parent: None,
            state: TaskState::Done,
            updated_at: Timestamp(1),
        })
        .unwrap();
        p.continuity_share("task", 100, false, Timestamp(2))
            .unwrap();
        p.continuity_share("memory", id.0, false, Timestamp(2))
            .unwrap();
        exchange(&mut p, &mut n);
        let remote_task = n.tasks().unwrap()[0].record.id;
        p.add_said(100, Timestamp(3), &crate::Said::Reply("x".repeat(MAX_ITEM)))
            .unwrap();
        p.forget(id, Timestamp(4)).unwrap();
        let outgoing = remember(&mut p, "New fact from phone");
        p.continuity_share("memory", outgoing.0, false, Timestamp(4))
            .unwrap();
        let incoming = remember(&mut n, "New fact from computer");
        n.continuity_share("memory", incoming.0, false, Timestamp(4))
            .unwrap();
        // Exercise capture on both sides: request, exchange, and accept must all
        // tolerate the oversized item while delivering unrelated changes.
        exchange(&mut p, &mut n);
        exchange(&mut n, &mut p);
        assert!(
            n.recent(10, Timestamp(5))
                .unwrap()
                .iter()
                .all(|m| m.text != "Forget this shared fact")
        );
        assert!(
            n.recent(10, Timestamp(5))
                .unwrap()
                .iter()
                .any(|m| m.text == "New fact from phone")
        );
        assert!(
            p.recent(10, Timestamp(5))
                .unwrap()
                .iter()
                .any(|m| m.text == "New fact from computer")
        );
        let info = p.continuity_info().unwrap();
        let task = info.items.iter().find(|i| i.kind == "task").unwrap();
        assert!(task.problem.as_deref().unwrap().contains("192 KiB"));
        assert!(task.owned);
        assert!(p.continuity_share("task", 100, true, Timestamp(6)).is_err());
        assert!(p.continuity_writable("task", 100).is_ok());
        assert!(
            n.tasks().unwrap()[0].chat.is_empty(),
            "keep the last good replica, not a truncated chat"
        );
        // Deleting even the oversized item itself must still propagate.
        p.delete_task(100, Timestamp(7)).unwrap();
        exchange(&mut p, &mut n);
        assert!(
            n.tasks()
                .unwrap()
                .iter()
                .all(|t| t.record.id != remote_task)
        );
        assert!(
            p.continuity_info()
                .unwrap()
                .items
                .iter()
                .all(|i| i.problem.is_none())
        );
        assert_eq!(p.continuity_info().unwrap().pending, 0);
    }

    #[test]
    fn corrections_move_ownership_and_deletion_wins_over_stale_copies() {
        let (mut p, mut n) = pair();
        let id = remember(&mut p, "Dentist at 14:00");
        p.continuity_share("memory", id.0, false, Timestamp(10))
            .unwrap();
        exchange(&mut p, &mut n);
        let nid = n.recent(10, Timestamp(100)).unwrap()[0].id;
        assert!(n.correct(nid, "wrong", Timestamp(11)).is_err());
        let new = p.correct(id, "Dentist at 15:00", Timestamp(12)).unwrap();
        exchange(&mut p, &mut n);
        assert_eq!(n.get(nid).unwrap().unwrap().text, "Dentist at 15:00");
        p.continuity_share("memory", new.0, true, Timestamp(13))
            .unwrap();
        assert!(p.correct(new, "wrong", Timestamp(14)).is_err());
        let request = p
            .continuity_request(&identities().1, Timestamp(100))
            .unwrap();
        let lost = n
            .continuity_exchange(&identities().0, request.clone(), Timestamp(100))
            .unwrap();
        assert!(p.correct(new, "still wrong", Timestamp(15)).is_err());
        let retry = n
            .continuity_exchange(&identities().0, request, Timestamp(100))
            .unwrap();
        p.continuity_accept(&identities().1, retry, Timestamp(100))
            .unwrap();
        n.correct(nid, "Dentist at 16:00", Timestamp(16)).unwrap();
        exchange(&mut p, &mut n);
        assert_eq!(p.get(new).unwrap().unwrap().text, "Dentist at 16:00");
        p.forget(new, Timestamp(17)).unwrap();
        let raw: Option<String> = p
            .conn
            .query_row(
                "SELECT payload FROM continuity_records WHERE kind='memory'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(raw.is_none());
        exchange(&mut p, &mut n);
        assert!(n.recent(10, Timestamp(100)).unwrap().is_empty());
        p.continuity_accept(&identities().1, lost, Timestamp(100))
            .unwrap();
        assert!(p.recent(10, Timestamp(100)).unwrap().is_empty());
    }
    #[test]
    fn moved_waiting_task_has_chat_but_no_executable_approval() {
        let (mut p, mut n) = pair();
        p.save_task(&crate::TaskRecord {
            id: 10,
            title: "Email dentist".into(),
            parent: None,
            state: TaskState::Waiting(crate::WaitingCall {
                tool: "send_email".into(),
                arguments: "private approval".into(),
                until: Timestamp(99999),
            }),
            updated_at: Timestamp(2),
        })
        .unwrap();
        p.add_said(
            10,
            Timestamp(2),
            &crate::Said::User("Please email my dentist".into()),
        )
        .unwrap();
        p.continuity_share("task", 10, true, Timestamp(10)).unwrap();
        exchange(&mut p, &mut n);
        let task = n.tasks().unwrap().remove(0);
        assert!(matches!(task.record.state,TaskState::Failed(ref reason) if reason==REVIEW));
        assert_eq!(task.chat.len(), 1);
        assert!(
            p.add_said(10, Timestamp(11), &crate::Said::User("cannot edit".into()))
                .is_err()
        );
        n.delete_task(task.record.id, Timestamp(20)).unwrap();
        exchange(&mut p, &mut n);
        assert!(p.tasks().unwrap().is_empty());
        assert!(n.task_events_intact().unwrap());
    }
    #[test]
    fn invalid_page_rolls_back_and_recovery_never_steals_ownership() {
        let (mut p, mut n) = pair();
        let id = remember(&mut p, "Important fact");
        p.continuity_share("memory", id.0, true, Timestamp(10))
            .unwrap();
        let mut req = p
            .continuity_request(&identities().1, Timestamp(10))
            .unwrap();
        let mut bad = req.page.changes[0].clone();
        bad.sequence += 1;
        bad.id = "invalid".into();
        req.page.changes.push(bad);
        assert!(
            n.continuity_exchange(&identities().0, req, Timestamp(10))
                .is_err()
        );
        assert!(n.recent(10, Timestamp(100)).unwrap().is_empty());
        exchange(&mut p, &mut n);
        p.continuity_enable(&identities().1, false).unwrap();
        let recovered = p.continuity_recover("memory", id.0, Timestamp(11)).unwrap();
        assert_ne!(recovered, id.0);
        assert!(p.get(id).unwrap().is_none());
        p.correct(MemoryId(recovered), "Recovered correction", Timestamp(12))
            .unwrap();
        p.continuity_enable(&identities().1, true).unwrap();
        exchange(&mut p, &mut n);
        assert!(n.recent(10, Timestamp(100)).unwrap().is_empty());
    }
    #[test]
    fn sensitive_or_proposed_facts_cannot_be_shared() {
        let (mut p, _) = pair();
        for sensitive in [false, true] {
            let mut v = Provenance::from_user(Timestamp(1));
            if sensitive {
                v.privacy = Privacy::Sensitive
            };
            let id = p
                .remember(NewMemory {
                    text: format!("private {sensitive}"),
                    status: if sensitive {
                        Status::Confirmed
                    } else {
                        Status::Proposed
                    },
                    provenance: v,
                })
                .unwrap();
            assert!(
                p.continuity_share("memory", id.0, false, Timestamp(10))
                    .is_err()
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod recovery_tests {
    use super::*;
    use pr3_memory::{NewMemory, Provenance};
    #[test]
    fn restart_lost_ack_pagination_and_replay_preserve_deletions() {
        for seed in [1u64, 2, 7] {
            let dir = std::env::temp_dir()
                .join(format!("pr3-sync-restart-{}-{seed}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("phone.sqlite");
            let a = "a".repeat(64);
            let b = "b".repeat(64);
            let mut p = SqliteMemory::open(&path).unwrap();
            let mut n = SqliteMemory::open_in_memory().unwrap();
            p.continuity_init(&a).unwrap();
            n.continuity_init(&b).unwrap();
            p.continuity_enable(&b, true).unwrap();
            n.continuity_enable(&a, true).unwrap();
            for i in 0..70 {
                let id = p
                    .remember(NewMemory {
                        text: format!("unique-memory-{seed}-{i}"),
                        status: Status::Confirmed,
                        provenance: Provenance::from_user(Timestamp(1)),
                    })
                    .unwrap();
                p.continuity_share("memory", id.0, i % 3 == 0, Timestamp(2))
                    .unwrap();
            }
            let first = p.continuity_request(&b, Timestamp(3)).unwrap();
            assert_eq!(first.page.changes.len(), 64);
            let lost = n
                .continuity_exchange(&a, first.clone(), Timestamp(3))
                .unwrap();
            drop(p);
            let mut p = SqliteMemory::open(&path).unwrap();
            p.continuity_init(&a).unwrap();
            // Randomly selected deleted records include ones whose ownership moved.
            for m in p.recent(100, Timestamp(4)).unwrap() {
                if m.id.0 as u64 % 3 == seed % 3 {
                    p.forget(m.id, Timestamp(4)).unwrap();
                }
            }
            for _ in 0..10 {
                let r = p.continuity_request(&b, Timestamp(5)).unwrap();
                let reply = n.continuity_exchange(&a, r, Timestamp(5)).unwrap();
                p.continuity_accept(&b, reply, Timestamp(5)).unwrap();
            }
            // Delayed traffic from before deletion cannot restore anything.
            n.continuity_exchange(&a, first, Timestamp(6)).unwrap();
            p.continuity_accept(&b, lost, Timestamp(6)).unwrap();
            let mut left: Vec<_> = p
                .recent(100, Timestamp(7))
                .unwrap()
                .into_iter()
                .map(|m| m.text)
                .collect();
            left.sort();
            let mut right: Vec<_> = n
                .recent(100, Timestamp(7))
                .unwrap()
                .into_iter()
                .map(|m| m.text)
                .collect();
            right.sort();
            assert_eq!(left, right, "seed {seed}");
            assert!(left.len() < 70);
            assert_eq!(p.continuity_info().unwrap().pending, 0);
            drop(p);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn conflicting_versions_and_bad_ack_are_atomic() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let mut p = SqliteMemory::open_in_memory().unwrap();
        let mut n = SqliteMemory::open_in_memory().unwrap();
        p.continuity_init(&a).unwrap();
        n.continuity_init(&b).unwrap();
        p.continuity_enable(&b, true).unwrap();
        n.continuity_enable(&a, true).unwrap();
        let id = p
            .remember(NewMemory {
                text: "safe-original".into(),
                status: Status::Confirmed,
                provenance: Provenance::from_user(Timestamp(1)),
            })
            .unwrap();
        p.continuity_share("memory", id.0, false, Timestamp(2))
            .unwrap();
        let r = p.continuity_request(&b, Timestamp(3)).unwrap();
        let mut bad = r.clone();
        bad.cursor = 99;
        assert!(n.continuity_exchange(&a, bad, Timestamp(3)).is_err());
        assert!(n.recent(5, Timestamp(4)).unwrap().is_empty());
        n.continuity_exchange(&a, r.clone(), Timestamp(4)).unwrap();
        let mut conflict = r;
        conflict.page.changes[0].sequence += 1;
        conflict.page.changes[0].payload = conflict.page.changes[0]
            .payload
            .as_ref()
            .map(|s| s.replace("safe-original", "tampered"));
        assert!(n.continuity_exchange(&a, conflict, Timestamp(5)).is_err());
        assert_eq!(n.recent(5, Timestamp(6)).unwrap()[0].text, "safe-original");
    }
}
