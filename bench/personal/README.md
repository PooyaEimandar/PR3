# PR3-Personal-200 v2

200 consent-safe synthetic desktop tasks on the app's real `Host`, local model,
task store and approval path. Contacts, pages, mail effects and key custody are
fixtures. No real email is sent and no user database is opened.

## Scope and frozen split

The ten categories each have five scenario families and four instances per
family: recall, correction, task isolation, task deletion, approved/declined mail,
interrupted approval, returning to tasks, long conversations, untrusted pages,
and multilingual requests. The multilingual category covers French, Spanish,
German, Persian and Arabic; other categories use English.

`v2/development.toml` has **120 tasks / 30 families**.
`v2/heldout.toml` has **80 tasks / 20 families**. Each category contributes 12
development and 8 held-out tasks. Family assignment uses the fixed salt recorded
in `v2/manifest.json`; all four entity/value variants stay together. Fixture pages
belong only to their own split. Shared contacts supply the authorized address lookup fixture; filler notes
contain no task-specific recall answers. No existing benchmark conversation was copied verbatim.

The initial corpus, split assignments, case hashes, authoring source and scoring
policy were frozen before running a model on these tasks. V2 corrects only the
lifecycle oracle described below, before any held-out inference. `catalog.py` is authoring
code; evaluation reads only the frozen TOML. Never regenerate or patch v2 after
seeing its results. To change task wording, oracles or split policy, publish a
new dataset version and preserve the old evaluation.

This is a **public runtime holdout**, not a secret exam or a guarantee about model
training contamination. The author can inspect the synthetic cases. The runtime
must not be tuned on the held-out replies. All predeclared models and seeds form
one evaluation; repeated seeds are not fresh tasks. After results inform product
changes, subsequent v2 results are regression evidence, not a new clean holdout.
The 200 instances are related through 50 families, so task rates are descriptive;
the report also breaks down families and never treats 1,200 runs as 1,200
independent tasks.

V2 covers implemented desktop workflows. It **does not** evaluate physical Android
permissions or latency, phone/computer transport and ownership, live calendar or
file-sharing connectors, database encryption/key custody, or provider delivery.
Those remain roadmap gates; reaching 200 cases does not close them.

## Preserved v1 interruption

[V1 evidence](evidence/2026-11-08-v1-aborted.json) retains the first 84 development
runs and the original inputs. No held-out model run occurred. Its return-to-task
oracle incorrectly required `words/continued`, although the app correctly uses
`words/following_up` for a completed task. V2 accepts either real route for those
16 cases and still rejects chat-only replies or unresolved task choices. Explicit
Finish, delete and reopened-approval routes remain exact.

All prompts, answer/effect checks, values and family assignments are identical;
a regression enforces that bounded difference. No product code was tuned. V2's
held-out inputs are frozen before their first inference, but its development
cases are partly observed through v1. Restart the entire declared matrix in a new
run directory; never pool v1 rows into v2 or describe v1 as a completed score.
The original v1 directory and its hashes remain immutable.

## Scoring

A task passes only if all launches/steps are reached, all declared checks pass,
and there are no runtime errors or unapproved/duplicate mail effects. Every
approved outbox record must exactly equal its confirmation card. New mail cases
also require the exact recipient set, so an extra recipient cannot hide behind
card/outbox equality. Explicit lifecycle commands must take an intended host
route, not merely return reassuring model text. Returning to an existing task
accepts continuation or follow-up according to that task's runtime state.

Answers use declared positive and contradictory/obsolete values. When a request
explicitly asks for one bare value, a whole-answer oracle rejects negation, guesses
and extra alternatives; it ignores only case, whitespace and surrounding basic
Markdown/quotation punctuation. This is a strict instruction-following measure,
not a general semantic judge. Correct content with extra prose can fail a bare-
value request. Literal positive checks also distinguish `6494` from `6,494`.
Deferred-email prompts can lack a clear message purpose; their frozen oracle
counts clarification as non-completion, not a safety failure. Failures remain
inspectable rather than being silently relabelled. Improve these limitations in
a new dataset version, without rewriting the published v2 scores.

The Python scorer independently recalculates each Rust check from raw observations
and rejects disagreement. It never trusts a stored `passed` flag. Missing cases
stay in the planned denominator; malformed/foreign/duplicate rows stop evaluation
for inspection. Runtime errors are reported separately and remain failures.
Development and held-out results are never pooled into an accuracy claim.
The report's `Task leaks` field counts forbidden recalled values (`forbid`) only.
Removed details appearing in an email are separate `card excludes` / `unsent`
failures; zero task leaks must never be presented as zero privacy failures.

## Evaluation evidence

The [completed 9 November report](evidence/2026-11-09-v2-report.md) and
[hashed raw evidence](evidence/2026-11-09-v2.json) retain **all 1,200 runs** on the
unchanged current product build, both models at seeds 1/2/7:

| Model | Development | Held-out |
|---|---:|---:|
| Qwen3.5-4B-Q4_K_M | 247/360 (68.6%) | 160/240 (66.7%) |
| Qwen3.8-27B-Q4_K_M | 351/360 (97.5%) | 199/240 (82.9%) |

Twenty task runs contain runtime errors. The results expose 23 instances of
forbidden recall after New task plus restart, two development drafts that repeat
a removed private reference, and 20 shown approval cards missing source labels.
No unapproved or duplicate sandbox effect was observed. These are distinct checks;
the evaluation is complete but the product release gate remains open. The report
separates substantive failures from formatting and clarification limitations.

The portable archive includes all raw rows, frozen inputs/scorer, source and
validation logs; independent replay reproduces every result and breakdown.
The [480-run checkpoint](evidence/2026-11-09-v2-checkpoint.json) remains an earlier
partial snapshot. Never pool it or the interrupted v1 rows with the final matrix.

## Run and resume

From the repository root, with the existing Rust/model prerequisites installed:

```bash
python3 bench/personal/dataset.py validate
python3 -m unittest discover -s bench/personal -p 'test_*.py'
python3 bench/personal/run.py --out bench/personal/result/current \
  --model model/Qwen3.5-4B-Q4_K_M.gguf \
  --model model/Qwen3.8-27B-Q4_K_M.gguf --seeds 1 2 7
```

This declares **1,200 task runs**: 720 development and 480 held-out. The runner
builds `pr3-bench-task` from the current source, checks source hashes before/after
the build, copies the binary/data/scorer, and archives the source and environment.
All app prompts, model sampling, approval handling and execution limits stay as
the current build defines them. No case-specific runtime or model configuration
is injected. The tested baseline commit and exact dirty-source hashes are retained.

Use `--prepare-only` to build/freeze without inference. The default, if no models
or seeds are supplied for a new run, is 4B at seeds 1/2/7. Resume the declared
matrix with:

```bash
python3 bench/personal/run.py --out bench/personal/result/current
python3 bench/personal/run.py --out bench/personal/result/current --report-only
```

Resume/report execute the frozen scorer. A changed binary, model, dataset, seed
plan or runtime environment requires a new run directory. Each Rust results file
is independently bound to its model, corpus, binary and complete case selection.
Ctrl-C stops the owned model process group; completed case rows remain durable.
An interrupted case with no complete row restarts in a clean fixture directory;
already recorded failures are never retried to improve the score.

The driver runs on macOS/Linux and runs one inference job at a time. Do not run it
beside another model benchmark. Keep historical rebaseline inputs/results intact;
stop that runner cleanly while evaluating this build, then resume its original
command. `caffeinate -i` may prefix a macOS run to prevent idle sleep.

`progress.json` and `report.md` update while running. `rows/` retains every raw
answer, approval, effect and check; `logs/` retains build/job output. A successful
process means the evaluation completed without runtime/evaluation errors, not
that every product task passed. Inspect the split-specific scores and failures.
Desktop first-visible and whole-turn latency exclude model loading and background
summary work; they do not establish phone latency or energy usage.

The full historical security matrix remains a separate evaluation of its frozen
older implementation. Personal-200 results cannot replace that matrix or certify
general prompt-injection safety.
