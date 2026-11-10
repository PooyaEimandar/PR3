# Approval-source persistence (10 November 2026)

All **22/22 shown approval cards** retain the source site after restart; **22/24** full workflows pass on 4B/27B at seeds 1/2/7. Two 27B seed-1 workflows reread the page after restart, triggered the unchanged same-turn action block and produced no card or email effect. These remain planning failures; all observations are retained.

[Results and hashes](source-provenance-2026-11-10.json) ·
[Portable raw evidence](source-provenance-2026-11-10.tar.gz) ·
[Original Personal-200 report](../../personal/evidence/2026-11-09-v2-report.md)

| Model | Seed | Full workflows | Correct source / shown cards | Missing cards |
|---|---:|---:|---:|---:|
| Qwen3.8-27B | 1 | 2/4 | 2/2 | 2 |
| Qwen3.8-27B | 2 | 4/4 | 4/4 | 0 |
| Qwen3.8-27B | 7 | 4/4 | 4/4 | 0 |
| Qwen3.5-4B | 1 | 4/4 | 4/4 | 0 |
| Qwen3.5-4B | 2 | 4/4 | 4/4 | 0 |
| Qwen3.5-4B | 7 | 4/4 | 4/4 | 0 |

The original 24 runs had 20 approval cards with missing labels and four 27B
seed-1 runs without cards. The same corpus and independent scorer are reused
unchanged. A missing card remains a workflow failure, never a source-label
success. Each current-build case ran once; all failed checks and raw observations
are retained. The model planners, trusted gate, execution configuration and
workflow harness match the preceding planning/status build. This is focused
regression evidence, not a new held-out accuracy result.

The two remaining failures are source-to-approved-mail cases 2 and 4 on 27B
seed 1. No runtime errors, unapproved or duplicate effects, or forbidden
recipient/marker content were observed. Android acceptance remains open.

Source origins previously lived only in the core's in-memory tool history.
Restart correctly restores user/assistant history without replaying tools, but
that lost the approval labels. Schema v8 now stores the registered tool adapter's
bounded origin metadata atomically with its result. Both new proposals and
restored pending cards read labels from retained task history. They do not parse
page headings, tool arguments, model replies or summaries to invent sources.
Redirected, cached and failed reads retain the actual origin the adapter provides.

The card collects up to 64 unique labels, at most 256 characters each, across
the current task and its explicit follow-up chain (eight tasks total), with a
visible overflow notice. New tasks start without inherited labels. Split exchanges keep their own metadata;
deleting task history removes its origins. Selected paired-task transfers retain
metadata while pending actions still become review-required tasks, never approved
actions. Legacy web records and known untrusted results without origins show
“a web result whose source was not saved.” Missing historical origins cannot be
reconstructed safely from attacker-controlled text or the pre-redirect URL.

Eleven new tests cover database reopen and pending-card recovery; redirected,
failed and cached reads; folding and explicit follow-ups; New task and splitting;
deletion; legacy migration and malformed metadata; failed writes and retry saving;
paired-device task serialization; deduplication and bounds. The initial reopen
regression failed before production changes. A save failure stops further
inference/actions; retrying persistence does not execute the tool again. Host
tests retain the same-turn untrusted-source action block, require fresh approval,
verify decline produces no effect and approve exactly one sandbox email.

All **420 workspace tests pass**, with 12 existing opt-in checks ignored. Strict
Clippy, formatting, TypeScript and core `no_std` checks pass. An older migration
test directly read a v4 fixture with the latest reader before migration; its
expected legacy contents are now explicit and compared after migration. The
first model snapshot's source check stopped during setup after that test-only
correction, before any inference. The interruption, build logs, failed fixture
log and historical-run resume audits are preserved.

The manifest freezes source, binary, model and corpus hashes. The portable archive
contains raw rows, checks, extracted original comparison rows, source snapshot,
validation logs and an independent replay script. After extraction:

```sh
python3 driver/replay.py .
```

Approval, object binding, cancellation, idempotency, same-turn source guards and
execution limits remain intact. Restoring metadata grants no authority and does
not restore receipts, core tool history or trust flags. These desktop sandbox
checks do not certify Android contacts/mail-app handoff, physical-device
continuity or delivery. The separate historical security matrix resumed unchanged
and predates this implementation.
