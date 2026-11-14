//! Device-local, explicitly imported UTF-8 documents. No filesystem traversal or
//! remote fetches. Sources are immutable snapshots with never-reused IDs.
use crate::{SqliteMemory, error};
use pr3_memory::MemoryError;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MAX_DOCUMENT_BYTES: usize = 512 * 1024;
pub const MAX_LIBRARY_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DOCUMENTS: usize = 200;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: i64,
    pub name: String,
    pub bytes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Passage {
    pub source: String,
    pub document: i64,
    pub name: String,
    pub first_line: usize,
    pub last_line: usize,
    pub text: String,
}

impl SqliteMemory {
    pub fn documents(&self) -> Result<Vec<Document>, MemoryError> {
        self.conn
            .prepare("SELECT id,name,length(CAST(text AS BLOB)) FROM documents ORDER BY id DESC")
            .map_err(error)?
            .query_map([], |r| {
                Ok(Document {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    bytes: r.get::<_, u32>(2)? as usize,
                })
            })
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)
    }
    pub fn import_document(&mut self, name: &str, text: &str) -> Result<Document, MemoryError> {
        let invalid = |message: &str| MemoryError(message.into());
        if name.is_empty()
            || name.len() > 200
            || name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
        {
            return Err(invalid(
                "Use a filename without folders or control characters",
            ));
        }
        if ![".txt", ".md", ".markdown"]
            .iter()
            .any(|ext| name.to_lowercase().ends_with(ext))
        {
            return Err(invalid("Choose a UTF-8 .txt or Markdown document"));
        }
        if text.trim().is_empty()
            || text.len() > MAX_DOCUMENT_BYTES
            || text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err(invalid(
                "Document must be nonempty UTF-8 text, at most 512 KiB, without binary data",
            ));
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let documents = self.documents()?;
        if let Some(id) = self
            .conn
            .query_row(
                "SELECT id FROM documents WHERE name=?1 AND text=?2",
                params![name, text],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(error)?
        {
            return Ok(Document {
                id,
                name: name.into(),
                bytes: text.len(),
            });
        }
        if documents.len() >= MAX_DOCUMENTS
            || documents.iter().map(|d| d.bytes).sum::<usize>() + text.len() > MAX_LIBRARY_BYTES
        {
            return Err(invalid(
                "Document library is full (200 files / 16 MiB). Remove a file first.",
            ));
        }
        let tx = self.conn.transaction().map_err(error)?;
        tx.execute(
            "INSERT INTO documents(name,text) VALUES(?1,?2)",
            params![name, text],
        )
        .map_err(error)?;
        let id = tx.last_insert_rowid();
        let mut ordinal = 0;
        // Line ranges stay exact, including Unicode and very long lines.
        let mut chunk = String::new();
        let mut first = 1;
        let mut last = 1;
        let mut put = |body: &str, start: usize, end: usize| -> Result<(), MemoryError> {
            if body.trim().is_empty() {
                return Ok(());
            }
            ordinal += 1;
            tx.execute("INSERT INTO document_passages(document,ordinal,first_line,last_line,text) VALUES(?1,?2,?3,?4,?5)",params![id,ordinal,start as i64,end as i64,body]).map_err(error)?;
            Ok(())
        };
        for (offset, line) in text.split('\n').enumerate() {
            let number = offset + 1;
            if !chunk.is_empty() && (line.is_empty() || chunk.len() + line.len() > 1600) {
                put(&chunk, first, last)?;
                chunk.clear();
            }
            let mut segment = String::new();
            for ch in line.chars() {
                if segment.len() + ch.len_utf8() > 1600 {
                    put(&segment, number, number)?;
                    segment.clear();
                }
                segment.push(ch);
            }
            if !segment.is_empty() {
                if chunk.is_empty() {
                    first = number;
                } else {
                    chunk.push('\n');
                }
                chunk.push_str(&segment);
                last = number;
            }
        }
        put(&chunk, first, last)?;
        tx.commit().map_err(error)?;
        Ok(Document {
            id,
            name: name.into(),
            bytes: text.len(),
        })
    }
    pub fn search_documents(&self, query: &str) -> Result<Vec<Passage>, MemoryError> {
        if query.len() > 512 {
            return Err(MemoryError("Search is limited to 512 bytes".into()));
        }
        let tokens: Vec<_> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .take(24)
            .collect();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        // Treat FTS syntax as data, never an expression supplied by a model/user.
        let expression = tokens
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(" OR ");
        self.conn.prepare("SELECT p.document,p.ordinal,d.name,p.first_line,p.last_line,p.text FROM document_fts f JOIN document_passages p ON p.id=f.rowid JOIN documents d ON d.id=p.document WHERE document_fts MATCH ?1 ORDER BY bm25(document_fts),p.document,p.ordinal LIMIT 5").map_err(error)?
            .query_map([expression], passage_row).map_err(error)?.collect::<Result<Vec<_>,_>>().map_err(error)
    }
    pub fn document_passage(&self, source: &str) -> Result<Option<Passage>, MemoryError> {
        let parts: Vec<_> = source.split(':').collect();
        if parts.len() != 4 || parts[0] != "document" {
            return Ok(None);
        }
        let (Ok(id), Ok(ordinal)) = (parts[1].parse::<i64>(), parts[2].parse::<i64>()) else {
            return Ok(None);
        };
        self.conn.query_row("SELECT p.document,p.ordinal,d.name,p.first_line,p.last_line,p.text FROM document_passages p JOIN documents d ON d.id=p.document WHERE p.document=?1 AND p.ordinal=?2",params![id,ordinal],passage_row).optional().map(|passage| passage.filter(|p| p.source == source)).map_err(error)
    }
    pub fn remove_document(&mut self, id: i64) -> Result<bool, MemoryError> {
        self.conn
            .execute("DELETE FROM documents WHERE id=?1", [id])
            .map(|n| n > 0)
            .map_err(error)
    }
}
fn passage_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Passage> {
    let document: i64 = r.get(0)?;
    let ordinal: i64 = r.get(1)?;
    let name: String = r.get(2)?;
    let text: String = r.get(5)?;
    let digest = pr3_core::trusted::Hash::of(&[name.as_bytes(), text.as_bytes()]).to_hex();
    Ok(Passage {
        source: format!("document:{document}:{ordinal}:{}", &digest[..32]),
        document,
        name,
        first_line: r.get::<_, u32>(3)? as usize,
        last_line: r.get::<_, u32>(4)? as usize,
        text,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_quotes_unicode_and_deletion_survive_restart() {
        let dir = std::env::temp_dir().join(format!("pr3-docs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.sqlite");
        let mut db = SqliteMemory::open(&path).unwrap();
        let d = db
            .import_document(
                "permit.md",
                "# Permit\n\nThe new code is AZ-711.\nموعد التسليم الخميس",
            )
            .unwrap();
        assert_eq!(
            db.import_document(
                "permit.md",
                "# Permit\n\nThe new code is AZ-711.\nموعد التسليم الخميس"
            )
            .unwrap()
            .id,
            d.id
        );
        let hit = db.search_documents("AZ-711").unwrap().remove(0);
        assert_eq!(hit.first_line, 3);
        assert_eq!(hit.last_line, 4);
        assert_eq!(db.search_documents("الخميس").unwrap().len(), 1);
        assert!(db.search_documents("\" OR * NEAR(").is_ok());
        drop(db);
        let mut db = SqliteMemory::open(&path).unwrap();
        assert_eq!(
            db.document_passage(&hit.source).unwrap().unwrap().text,
            hit.text
        );
        assert!(db.remove_document(d.id).unwrap());
        drop(db);
        let mut db = SqliteMemory::open(&path).unwrap();
        assert!(db.search_documents("AZ-711").unwrap().is_empty());
        assert!(db.document_passage(&hit.source).unwrap().is_none());
        let new = db
            .import_document("permit.md", "Replacement code CT-992.")
            .unwrap();
        assert!(new.id > d.id);
        assert!(db.document_passage(&hit.source).unwrap().is_none());
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn source_ids_cannot_resolve_to_another_devices_document_with_the_same_row_id() {
        let mut a = SqliteMemory::open_in_memory().unwrap();
        let mut b = SqliteMemory::open_in_memory().unwrap();
        a.import_document("notice.txt", "The code is AA-111.")
            .unwrap();
        b.import_document("notice.txt", "The code is BB-222.")
            .unwrap();
        let source = a.search_documents("code").unwrap().remove(0).source;
        assert!(b.document_passage(&source).unwrap().is_none());
        assert!(a.document_passage("document:1:1").unwrap().is_none());
    }

    #[test]
    fn bounds_and_line_ranges_do_not_split_utf8_or_accept_paths() {
        let mut db = SqliteMemory::open_in_memory().unwrap();
        for (name, text) in [
            ("../secret.txt", "hello"),
            ("a.pdf", "hello"),
            ("a.md", "a\0b"),
            ("a.md", ""),
        ] {
            assert!(db.import_document(name, text).is_err());
        }
        assert!(
            db.import_document("a.txt", &"a".repeat(MAX_DOCUMENT_BYTES + 1))
                .is_err()
        );
        db.import_document("long.txt", &format!("{}\nLAST", "🦀".repeat(2000)))
            .unwrap();
        let hit = db.search_documents("LAST").unwrap().remove(0);
        assert!(hit.text.ends_with("LAST"));
        assert_eq!(hit.last_line, 2);
    }
}
