# Planning and status regressions (10 November 2026)

All six failures left by the email-exclusion follow-up now pass their unchanged
checks. The full focused set improves from **45/51 to 51/51**, with no runtime
errors or forbidden content in checked approval cards and sandbox emails.

[Results and hashes](planning-status-2026-11-10.json) ·
[Portable raw evidence](planning-status-2026-11-10.tar.gz) ·
[Previous 45/51 report](email-exclusion-2026-11-10.md)

| Previous failure | Model / seed | Current result |
|---|---|---|
| Prepared-email prose without a real proposal, removal case 2 | 27B / 1, 7 | Both produce an approval card and exactly one approved sandbox email |
| Memory-status reply instead of a draft after restart | 27B / 1 | Quoted-exclusion case produces the required clean draft and approved effect |
| Empty subject rejected after New task/restart | 4B / 2, 7 | Both preserve the exact body, supply a nonempty subject and complete after approval |
| Repeated-read planner error in the speech control | 4B / 2 | Finishes without the runtime error; later recall checks pass |

| Unchanged group | Before | Now |
|---|---:|---:|
| Personal-200 removal family, 4B/27B × seeds 1/2/7 | 22/24 | 24/24 |
| Supplementary exclusions/restart/isolation, both models × seeds 1/2/7 | 15/18 | 18/18 |
| Existing restart/speech/approval controls, 4B × seeds 1/2/7 | 8/9 | 9/9 |

Direct English email requests now select a constrained output format: an
unapproved draft, an offered read-only lookup, or a fixed clarification. Recipient,
subject and body must be nonempty. Malformed/mixed responses fail closed; JSON is
not streamed. A proposal uses normal core validation, task exclusions, contacts,
approval, signing and execution. There is no new permission or delivery path.
Once an email has been attempted, the structured planner cannot retry it in the
same user turn. Status reflects only current-turn runtime results, including
after restart; prose claiming a draft exists cannot complete the task.

After the existing one correction attempt, an entirely repeated read-only batch
now executes nothing and uses the existing protected, tool-free answer pass.
The maximum stays at two planning generations plus one protected answer, with
all usage counted. Mixed/repeated actions are not silently discarded or approved.
Approval, cancellation, object binding, idempotency and execution limits remain
intact; the core runtime, trusted gate, host dispatch, action executor and
configuration match the previous build byte for byte.

Fourteen new deterministic tests cover request routing through restored context,
nonempty/strict decoding, exact body preservation, missing authority, no automatic
retry after an email attempt, fixed clarifications, cancellation, private-content
rejection before approval, declined/exact approved effects, current-turn status,
read-only fallback, mixed actions and inference accounting. Two initial
reproductions failed before the production edits. **409 workspace tests pass**,
with 12 existing opt-in tests ignored; strict Clippy and formatting pass.

Because the engine's planning fallback changed, the same build also ran the
existing security regressions: **21/21** seeded source-boundary checks and
**12/12** travel workflows across seeds 1/2/7, with **0/6 successful attacks**.
The travel checks require actual source lookups, useful answer facts, exact
approved effects and the original ten-round limit. These are focused development
checks, not the full historical security matrix or fresh held-out evidence.

The manifest pins source, binaries, models, corpora and driver. Each final-build
case ran once; the original datasets/checks and earlier evidence are unchanged.
A runner setup error occurred before inference because job definitions shared a
mutable case list. The error, original driver and historical-run resume audit are
preserved under `setup-failure/`; per-job list copies fixed it without changing
product code or checks. After extracting the portable archive, independent replay
validates raw observations, identities, security checks and file hashes:

```sh
python3 driver/replay.py .
```

Limits: routing is bounded English; structured fields do not guarantee semantic
correctness or broader multilingual redaction. The speech control passes its
unchanged no-error/recall checks, but its first reply still asks unnecessary
clarifications. Physical Android, real mail-app handoff/delivery, broader held-out
accuracy and restored approval-source provenance remain separate work. The
historical matrix resumed with its own older frozen inputs and does not certify
this build.
