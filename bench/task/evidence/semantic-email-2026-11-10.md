# Category-aware multilingual email exclusions (10 November 2026)

The final build passes **45/45** focused workflow runs on the installed
Qwen3.5-4B-Q4_K_M model at seeds **1/2/7**: **27/27** new development runs and
**18/18** unchanged controls. No runtime errors, forbidden card/outbox content,
unapproved effects or duplicate effects were observed.

[Results and hashes](semantic-email-2026-11-10.json) ·
[Portable raw evidence](semantic-email-2026-11-10.tar.gz) ·
[V2 development corpus](../semantic-email-v2.toml) ·
[Frozen v1 development corpus](../semantic-email.toml)

| Group | Cases × seeds | Passed |
|---|---:|---:|
| Category exclusions in five languages | 5 × 3 | 15/15 |
| Clarification after restart | 1 × 3 | 3/3 |
| Corrected private values | 1 × 3 | 3/3 |
| Quoted-command control | 1 × 3 | 3/3 |
| Scoped income reinclusion | 1 × 3 | 3/3 |
| Existing literal/field/new-task exclusions | 3 × 3 | 9/9 |
| Existing restart/speech/approval recovery | 3 × 3 | 9/9 |

## Implementation and boundaries

The host recognizes a defined set of direct correction commands. A local vocabulary
maps financial, health, contact and access categories to known fields and aliases
in English, French, Spanish, Persian and Arabic. For example, a financial exclusion
covers supplied salary/income, bank account, card and tax fields. Explicitly allowing
income again leaves the other fields excluded, including later supplied values.
The latest explicitly re-included values are supplied as bounded planning context
after restart. Superseded values are not highlighted, separate exclusions still
win, and a reinclusion never authorizes an action.
No model interprets or authorizes these corrections, and no inference allowance or
execution limit increases.

The original task's user messages remain the source of truth. Rules are rebuilt
through folding and real database reopens; explicit retained follow-ups inherit
rules, and their corrections do not mutate the parent. Other tasks, deleted tasks,
assistant replies, tool results and summaries cannot add or revoke a restriction.
Repeated facts do not consume the 128-distinct-value bound or hide later corrections; exceeding the distinct-value bound still blocks approval. No schema or continuity payload changes are required.

Recognized missing or ambiguous targets block approval and ask for exact text.
A standalone quoted clarification can resolve one pending target, or name one of
several. It does not become an email command in unrelated conversation and cannot
silently remove the other restrictions in a category. Incomplete quoted field
data remains blocked even after clarification; explicitly revise the category
instruction if its scope needs to change.

Checks run before both new and restored approval cards and cover subject, body,
recipient name and address. They match resolved values, normalized numeric forms
and recognized field statements in a draft. Exact absence statements such as
“My salary was omitted” do not supply a private value. The signed/handed-off draft
is never silently rewritten. Safe revised drafts still require fresh approval;
declines have no effect, and approved sandbox records exactly match the card.

The core, trusted gate, action executor, execution configuration and shared engine
have unchanged hashes relative to the previous source-planning build. The structured
email planner changes only its policy text: include requested permitted facts in
the body, honor scoped reinclusion, and do not substitute an invented attachment.
A full-file comparison confirms its grammar, decoder, tool permissions, generation
count and attempt limits remain unchanged. The host comparison verifies that production
changes are limited to routing pending clarifications using task context. Approval dispatch
and execution code remain unchanged.

## Validation and preserved observations

All **443 workspace tests pass**, with **17 new Rust regressions** and 12 existing
opt-in tests ignored. All **21 Python harness tests** pass, including two new
checks for versioned numeric alternatives. Strict Clippy and formatting pass on
the final source. The regressions cover multilingual concepts, corrected/translated fields,
source/quote boundaries, excessive removal, Unicode/numeric forms, clarification,
scoped reinclusion, folds/restart/follow-ups/deletion, failed saves, restored unsafe
cards, decline and exact fresh approval.

Six new tests failed before implementation; two additional boundary checks failed
before context/incomplete-value handling, and the scoped-reinclusion regression
failed before field exceptions were implemented. The initial model candidate was
stopped during code review to address that exception and protect categories during
clarification. Its **5 recorded runs (5 passed)**, source snapshot,
inputs and interruption/resume audits are retained under `initial-candidate/`.
A second candidate recorded **24 runs (22 passed)** before being
stopped for an oracle correction. The frozen check expected `74200`, while a
correctly approved draft rendered the income as `$74,200`. V1's failed score and
raw card remain under `v1-candidate/`. That candidate also recorded a genuine seed-2
income omission; the oracle correction does not turn it into a pass. V2 explicitly lists equivalent grouped and
Persian/Arabic digit forms in both positive and negative checks; it changes no
prompt, contact, approval decision or independent scorer. New Python regressions
verify that version boundary and detect formatted disclosures. These results are
not pooled with v2 or retroactively marked as passes. The final product also fixes
repeated-fact capacity and preserves late legacy-field corrections; both regressions
failed before that change.

A third candidate recorded **37 runs (36 passed)**.
Its seed-2 income-reinclusion case confirmed the genuine omission: the draft retained
the public reference and excluded the bank account, but left out the income the
user had explicitly allowed back in. This raw failure remains under
`reinclusion-candidate/`. A failing regression led to the bounded latest-value
planning note described above, including a database-reopen check. A fourth, context-only candidate still failed the same one case before stopping;
it is preserved under `context-candidate/`. A host integration test verifies that
the re-included value reaches the model after restart. The final change also makes
the drafting policy explicit about requested permitted body content and invented
attachments. The final build runs that case first with the same v2 input and checks.

The final evaluation froze source, binary, model, corpus and driver hashes before
inference. Every one of the 45 planned model/seed/case combinations ran once.
The existing exclusion/compatibility corpora and independent scorer are unchanged. The new semantic-email corpus is explicitly versioned as v2.
The archive contains raw rows, logs, source snapshots, identities, validation logs
and a replay script. After extraction:

```sh
python3 driver/replay.py .
```

## Limits

This is deterministic, bounded category/field matching and development regression
evidence, not unrestricted semantic redaction or fresh held-out accuracy. It does
not cover arbitrary command phrasing, unlabelled narratives or arbitrary translations
and paraphrases of private values. Reinclusion uses a named field/category; allowing
one literal alone does not override a category. The existing speech control checks
recall and lack of runtime errors, not ideal conversational wording. The checks do
not establish full prose accuracy: a preserved v1 draft said “Please find attached”
without an attachment. The final policy addresses that observed substitution; these focused checks still
do not establish general prose or attachment accuracy.

No 27B run, physical Android acceptance or real email delivery is claimed here.
The historical matrix was already stopped before the initial candidate; its original
runner was restarted afterward, paused for the subsequent evaluations and restarted after each one.
It retains its older frozen build and is separate from this current-build evidence.
