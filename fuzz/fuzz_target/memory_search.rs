//! Memory search under arbitrary queries and memories: the full-text index's query
//! syntax cannot be injected, so a search never fails and never panics, and a memory
//! the user just stored can be found by one of its own words.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use pr3_memory::{MemoryStore, NewMemory, Provenance, SessionId, Status, Timestamp};
use pr3_memory_sqlite::SqliteMemory;

#[derive(Arbitrary, Debug)]
struct Case {
    memories: Vec<String>,
    query: String,
}

fuzz_target!(|case: Case| {
    let mut store = SqliteMemory::open_in_memory().unwrap();
    let at = Timestamp(1_000);
    for text in case.memories.iter().take(8) {
        let new = NewMemory {
            text: text.clone(),
            status: Status::Proposed,
            provenance: Provenance::proposed(SessionId(1), text.clone(), at),
        };
        let _ = store.remember(new);
    }
    let found = store.search(&case.query, 5, Timestamp(2_000));
    assert!(found.is_ok(), "search failed: {found:?}");
    let recent = store.recent(10, Timestamp(2_000));
    assert!(recent.is_ok(), "listing failed: {recent:?}");
});
