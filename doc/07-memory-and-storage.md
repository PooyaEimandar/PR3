# Memory and storage

What PR3 remembers and where it keeps it. The memory model (provenance, proposals and confirmations, corrections, forgetting) lives in `src/memory/`; the stores (memories, tasks, documents, research evidence, continuity, settings) in `src/store/`, on SQLCipher through its C API. A database written by any released version must open, verify and keep working under every later one ([DATA-02](17-delivery-plan.md), [APP-08](17-delivery-plan.md)).

## Memory

A memory is text with provenance: its source (a conversation, with the user's own words of that turn as the excerpt, or the user directly), principal (`user` or the person it is about), creation and validity times, confidence (0–100; 70 for a proposal, 100 for the user's entry), privacy (`personal` or `sensitive`; sensitive never leaves the device), consent (`device_only`), retention (`keep` or until a time). The model never decides what is true about the user: what it writes is **proposed**; the user's entries and corrections are **confirmed** and supersede proposals. A memory is live unless forgotten, corrected or past its retention.

`src/memory/` holds the types, `possible_updates` (conservative correction candidates: shared subject words, never permission to overwrite) and retrieval:

- **Terms.** Negative tails cut (`. do not `, `; do not `, and more in correction and quote mode), text after `sources only:` cut, words containing `://` dropped; split on non-alphanumerics; words over two characters; about 45 stop words removed; ASCII words of five or more letters lose one suffix (`ing`, `ers`, `ies`, `ed`, `er`, `ly`, `s`) keeping three, then a trailing `e` if four remain; sorted and deduplicated.
- **Sentences.** Split after newline, `;`, `؛`, `。`, or after `.`, `!`, `?` followed by whitespace or the end.
- **Scores.** `passage_score = 4 × best(two adjacent sentences) + whole-text matches + 12 × option_score`; `select_parts` is greedy over new terms covered, score, lowest index.
- **References.** `quote_ref` ([part 06](06-inference.md#evidence-and-the-report-grammar)) and the research references `s:`/`c:` (the first 16 hex digits of `pr3_hash_of(["pr3-research-reference-v1", kind, task as u64 LE, id])`), resolved by scanning and refusing ambiguity.

All of it works on slices in a caller's scratch arena; term sets are sorted arrays, coverage sets are bitsets per part.

## Stores

One SQLCipher database, `memory.sqlite`, behind `storage-key.lock`, holds memories, tasks, documents and research. The schema is built by eleven migrations; the schema version is `PRAGMA user_version`; a newer database is refused (`written by a newer PR3`); every migration runs in its own transaction; `foreign_keys` and `secure_delete` are on.

| Version | Tables and changes |
| --- | --- |
| 1 | `sources`, `memories` (status, provenance columns, `supersedes`/`superseded_by`, `deleted_at`), partial index on live `normalized`, `memories_fts` (FTS5, external content, `porter unicode61`) with its triggers, `memory_tombstones` |
| 2 | FTS `secure-delete` and `optimize` |
| 3 | `tasks`, `messages` (cascade), `task_events` (hash chain) |
| 4 | `tasks.summary`, `tasks.summarized` |
| 5 | Rebuild of the task tables adding the `finished` state and event |
| 6 | Continuity: `continuity_state`, `continuity_records`, `continuity_journal`, deletion triggers |
| 7 | `task_selection` (current task or follow-up, never both) |
| 8 | `messages.sources` |
| 9 | `documents`, `document_passages`, `document_fts` (`unicode61`, secure delete) |
| 10 | `research_runs`, `research_candidates`, `research_sources`, `research_settings` |
| 11 | `chat_requests` |

The migration SQL lives in `src/store/migrations.sql.h`. Fixture databases at each schema version, under `test/fixtures/store/`, are opened and migrated by the tests.

**Statements.** The stores prepare every statement once at open into a table indexed by an enum and reset and rebind them per use; text comes back as slices valid until the next step of the statement and is copied only into the caller's arena.

**SQLCipher's memory.** SQLCipher is started with `SQLITE_CONFIG_HEAP` over a PR3-owned region (memsys5, a buddy allocator in SQLite itself), sized by configuration, so the database's memory is bounded and reported beside the PR3 arenas, and a hostile database cannot grow the process. Page cache and lookaside sizes are fixed at open.

### Memories

- **Search.** Terms split on non-alphanumerics, lowercased, longer than one character, not one of 41 stop words, at most 32; the FTS query quotes every term (`"t1" OR "t2"`), so FTS syntax cannot be injected; ordered by `bm25` then newest; only live rows.
- **Deduplication.** A live memory with the same normalized text (curly apostrophes straightened, whitespace collapsed, trailing `.`/`!` removed, lowercased) and principal is returned instead of a new one, and confirmed if the new one is.
- **Correct.** In one transaction: insert a confirmed memory that supersedes the old one, mark the old one and matching proposals superseded, move its continuity identity, clear every task summary.
- **Forget.** Walk the supersession chain, blank text, normalized text and principal, set `deleted_at`, clear a source excerpt no live memory still uses, write a tombstone, clear every summary. The FTS index uses secure delete, and tests scan the raw file for forgotten words.

### Tasks

States: working, waiting (tool, arguments, until), done, finished, failed (reason), expired. Every change of state appends a task event to one global hash chain: `hash = pr3_hash_of([previous hash as hex text, task as i64 BE, at as i64 BE, event, parent as i64 BE or empty])`, verified by walking every event in order. Messages (user, reply, tool, failed, with tool sources as JSON) cascade with their task; events stay. Splitting moves the latest messages into a new task; finished tasks are erased `keep_days` (30) after they last changed.

### Documents

Text and Markdown only (`.txt`, `.md`, `.markdown`), at most 512 KiB each, 16 MiB and 200 documents in all, names up to 200 bytes without path separators, CRLF normalized, identical (name, text) pairs stored once. Passages of at most 1600 bytes split at blank lines; source ids `document:<doc>:<ordinal>:<first 32 hex of sha256(name, text)>`, re-verified on lookup; search with at most 24 quoted terms, `bm25`, five hits. Removing a document removes the tasks that depend on it.

### Research evidence

Per task: one run (topic of 1–512 bytes, fixed once research begins; web permission; search state), at most 8 candidates, at most 96 sources and 128 KiB (16 MiB in all), each source's text at most 3000 bytes; evidence whose backing document was removed is hidden. Provider `tavily` or `brave`; API keys of 16–512 printable ASCII characters, stored only in encrypted storage and wiped from memory after use.

### Continuity

Sharing selected items with one paired device ([part 09](09-protocol-and-hybrid.md)): records keyed by a global id (`<creator device>:<32 random hex>`), with owner, epoch and revision; a journal gives each change a sequence; pages of at most 64 changes and 512 KiB, items of at most 192 KiB. Only confirmed, personal, unexpired memories and non-working tasks can be shared, stripped of excerpts, ids, parents and summaries. Applying a page is one transaction: sequences strictly increase, the acknowledgement cannot exceed what was sent, a newer version is accepted only from the current owner or as a transfer of ownership to this device, deletion wins over stale copies, and an installed task that was working or waiting becomes failed for review, because approvals never transfer. Recovery is allowed only while unpaired.

## Keys and encryption

| Rule | Detail |
| --- | --- |
| The key | 32 random bytes applied raw as `PRAGMA key = "x'<64 hex>'"` (no KDF), `cipher_compatibility = 4`, `temp_store = MEMORY`, then a probe query |
| Integrity | `PRAGMA integrity_check` returns `ok` and `PRAGMA cipher_integrity_check` returns nothing |
| Recovery code | `PR3` followed by 16 groups of `-xxxx`; parsing strips whitespace and dashes, at most 128 characters |
| Lock file | `<path>.lock` (0600, symlinks refused); a new file created exclusively (0600) and synced; an existing empty file refused; through `src/platform/` file primitives |
| Plaintext databases | Migrated through a staged `.encrypting` copy with `sqlcipher_export`, verified, synced, renamed over the original, directory synced |
| Secrets in memory | Key bytes, hex, recovery code and the SQL key string live in the secret arena and are wiped on every exit path |

**SQLCipher build.** Version 4.20.0 on SQLite 3.54.1 (3.54.0 or newer is required for the WAL disclosure fix). The crypto provider is CommonCrypto on Apple and SQLCipher's libtomcrypt provider elsewhere, which writes the same file format with a far smaller dependency than OpenSSL ([D-13](19-decisions-and-risks.md)).

**Key custody.** On Apple the key is a Keychain item (service `<bundle id>.storage`, account `memory-v1`, this device only, when unlocked); on Android it is wrapped with an AES-GCM Keystore key (`pr3-memory-wrap-v1-<uuid>`, record `{version:1, alias, wrapped}`); on Windows it is protected with DPAPI bound to the user; on Linux it is kept in the Secret Service (libsecret). All four sit behind one platform interface and fail closed when unavailable: the storage stays locked rather than falling back to a weaker store ([PLT-02](17-delivery-plan.md)).

## Configuration

`config/pr3.toml` holds `[model]` (device-class choices and memory thresholds), `[traces]`, `[tasks]`, `[web]`, `[node]`, and `[models."<id>"]` entries pinned to a repository revision, file, SHA-256 and size. The built-in copy is compiled in; a copy in the app's data folder overrides it; `PR3_MODEL` overrides the model. A broken override falls back to the built-in file with web reading off and tasks kept forever, and reports the problem.

The C reader accepts the subset the file uses (tables, quoted keys, strings, integers, booleans, comments), refuses unknown keys, limits the file to 64 KiB, and is fuzzed. No general TOML library is linked.

**Models.** Downloads resume from `<file>.<sha256>.partial` with `Range` (206 must start at the right offset, 200 restarts, 416 means complete), are hashed while they stream (1 MiB buffer), and are renamed into place only when the SHA-256 matches; on a metered connection a phone waits unless the user allowed mobile data. A model fits a device when its size is at most 75% of memory on a computer and 50% on a phone; the choice order is `PR3_MODEL`, the user's selection if it fits, the configuration's recommendation, then none.

---

[Back to the index](../README.md)
