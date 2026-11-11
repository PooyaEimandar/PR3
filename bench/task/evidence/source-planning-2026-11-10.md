# Resumed email planning (10 November 2026)

Both remaining planning failures now pass. The unchanged source-to-approved-email
family improves from **22/24 to 24/24** on 4B/27B at seeds 1/2/7, with **zero page
reads or fetches after restart**, **24 correctly labeled approval cards**, and
**exactly one approved sandbox email per workflow**. All nine existing 4B
restart/speech/approval controls pass too. No runtime errors or forbidden
recipient/marker content were observed on the final build.

[Results and hashes](source-planning-2026-11-10.json) ·
[Portable raw evidence](source-planning-2026-11-10.tar.gz) ·
[Previous 22/24 report](source-provenance-2026-11-10.md)

| Group | Previous | Final build |
|---|---:|---:|
| 27B source workflows, seed 1 | 2/4 | 4/4 |
| 27B source workflows, seeds 2/7 | 8/8 | 8/8 |
| 4B source workflows, seeds 1/2/7 | 12/12 | 12/12 |
| Existing 4B restart/speech/approval controls | — | 9/9 |

The two failures were cases 2 and 4 at 27B seed 1. Their follow-up began with
“Now email …”, which missed the structured email-request route. The general
planner reread the source, so the existing same-turn source/action guard correctly
refused the email. The bounded recognizer now accepts leading `Now` or `Now,`
within its existing two-introductory-phrase limit. The initial planning
instructions and grammar are unchanged; the same details can now produce an
unapproved draft without a new source read. Runtime completion handling recognizes
the same request, so an old result or a prose-only draft claim cannot finish it.

The initial candidate fixed both rereads but passed only **1/2** target workflows:
case 4 proposed a second email during post-result general planning. The scripted
approver approved both cards, producing two sandbox effects and failing the
unchanged duplicate/exact-effect checks. This was not an unapproved or external
send. That candidate, its two raw rows, source snapshot, hashes and resume audit
are retained under `initial-candidate/`; no failing observation was discarded.

The final adapter detects an email attempt in native assistant-call records after
the current direct email request. It then offers no tools and uses the engine's
existing protected, tool-free answer pass, omitting action-planning directives.
It does not reset trust state or increase budgets. Tool calls returned by a final
answer still fail the engine's existing guard. Success, decline, failure and an
uncertain outcome all end that planning phase. A new user request can propose
another draft, subject to normal approval. Source text cannot impersonate an
assistant-call record, and unrelated workflows keep their original tool offers.

Six new regressions cover routing, intent/authority boundaries, real database
reopen with original facts and source labels, exact approved and declined effects,
explicitly requested refreshes, post-email tools and truthful current-turn status.
The refresh test fetches changed data and confirms that the source-reading turn
still cannot execute an email; a later user request requires fresh approval.
Four routing/restart/status tests failed before the prefix fix, and a separate
post-email tool-offer test failed before finalization changed. All **426 workspace
tests pass**, with 12 existing opt-in checks ignored; strict Clippy and formatting
pass. The core runtime, trusted gate, host dispatch, action executor, execution
configuration and shared engine match the previous build byte for byte.

The final evaluation froze source, binary, model, corpus and driver hashes before
inference. The two failing cases ran first; each of the 33 final-build model/seed/case combinations ran once.
Personal-200 inputs/checks and previous evidence are unchanged. The additional
controls exercise ordinary email and unrelated workflows after the result-handling
change. The speech control remains limited by its frozen no-error/recall checks;
passing them is not a claim that its wording or clarifications are ideal.

After extracting the portable archive, independently replay raw observations,
identities and hashes, including the previous baseline and failed candidate:

```sh
python3 driver/replay.py .
```

Limits: this is known-case regression evidence, not fresh held-out accuracy or
general multilingual intent recognition. Needed lookups and user-requested
refreshes remain possible; there is no general ban on rereading pages. No source
bodies, tool history, trust flags or approval receipts are replayed after restart.
Desktop sandbox results do not certify Android handoff, real delivery or
physical-device continuity. The historical security matrix resumed with its own
older frozen build and remains separate from this evaluation.
