# Frozen research evaluation: test handoff, 2026-11-16

The frozen v1 corpus and matched upstream harness are implemented. **Live Tavily testing and the full 192-run comparison are complete; general live research acceptance failed.** See the [live findings and timings](2026-11-16-live-report.md) and [completed matrix with source review](2026-11-16-held-out-report.md). Do not interpret smoke checks as a research-quality or release result.

## Prepared

- Sixteen fresh synthetic scenario families; four offline resumed turns. Both models, seeds 1/2/7, PR3 and upstream OpenJarvis: 192 scenario runs / 240 turns.
- The actual upstream `OrchestratorAgent` at `b3b7f07efda4ea30fe61f25309d4af5b9dba5c34`, using shared model bytes, research tools, source snapshots and execution limits. No action connector or new approval authority.
- Separate metrics for facts, authentic quotations, explicit reasoning checks, runtime failures and warm-turn latency. Source-by-source review of all 240 produced answers is complete. This is not a comparison of native retrieval stacks.
- An isolated debug macOS bundle, `target/debug/bundle/macos/PR3 Research Check.app`, identifier `ai.pooya.pr3.research-check`. The local test configuration selects 4B and disables node serving. Temporary local conversation tracing used for acceptance is now disabled. Regular PR3 storage is separate. The app is ad-hoc signed, not a notarized release build.

## Checks

Workspace unit/integration tests: **384 passed, 9 ignored**. The concurrent app build caused an artifact lookup failure in the initial doctest phase; the sequential workspace doctest rerun passed. Strict workspace Clippy, formatting and TypeScript checks passed. Regression coverage includes two bridge tests for execution ceilings and frozen network scope, plus nine TypeScript tests for corpus integrity, scoring, metadata leakage, Markdown citation formatting, duplicate/error accounting and latency quantiles. The four existing Stop tests also pass. Production prompts, approval checks and execution limits remain unchanged.

Initial smoke inference exposed a scorer-format bug: a valid quotation under a bold `Source` label was rejected. The scorer now accepts equivalent Markdown wrappers, with a regression test. The [two smoke attempts](2026-11-16-research-smoke.tar.gz) retain the original model outputs; no production prompt or frozen scenario was changed. The final smoke rerun has four recorded rows, zero runtime errors, PR3 2/2 and OpenJarvis 1/2 automatic checks. A shortened source ID in an OpenJarvis final answer correctly fails citation checks even though the preceding report tool had succeeded. Scoring also groups different parts of the same web page as one source. Smoke scores and timings are not held-out findings.

## Live app acceptance procedure

1. Launch the isolated app. In Settings → Web, enable web reading and research, choose Tavily and save the key. The key must stay in the app; do not copy it into chat or logs.
2. Use a public topic unrelated to the frozen cases, for example: “Compare SQLite WAL mode and rollback journaling for concurrent readers and writers. Use official SQLite documentation, cite the evidence, and state limitations.”
3. Check the selected provider's actual result, exact source quotations, relevance, unsupported conclusions and honest missing-evidence messages. Record total turn time, first visible result, inference/tool durations and token counts from the isolated app's local trace.
4. Restart and resume the same task; inspect its saved sources. Disable web reading and verify that the app does not imply a fresh lookup occurred. Repeat warm turns separately from cold model startup. Public test chats are recorded in local plaintext diagnostics; the API key is not included.
5. End live inference before running the frozen matrix. Independently rescore the retained artifacts, review conclusions against source snapshots, and publish per-model/arm quality and latency with all errors included. Physical Android acceptance stays with the user.

The user entered the key and the procedure was exercised. Source inspection and a short sourced answer worked; four of five diagnostic turns failed from context exhaustion. The matrix, semantic review and final comparison report are complete. Power changed during the run, so a controlled latency repeat remains necessary for stronger performance claims. The synthetic results do not erase the live acceptance failures. The key is retained, web reading is restored to enabled, the test app is closed and temporary tracing is disabled.
