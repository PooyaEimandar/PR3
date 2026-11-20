# Research quotation and interpretation checks — 20 November 2026

Research findings can now answer directly with checked quotations, and inference keeps each source reference beside its exact excerpt. Optional anchors detect reference/text mismatches without silently substituting a citation. Conservative checks reject strong claim/quotation mismatches, certain false source attributions and affirmative current-validity claims backed by explicit past-year-only windows. Current-fact requests retain the trusted runtime date after context trimming. **These are bounded quality checks, not general semantic entailment or a guarantee of correct interpretation.**

## Implementation and regression coverage

- Optional finding analysis lets direct factual answers use exact quotations without adding an unnecessary paraphrase. Narrow user-selected “quote only” requests suppress generated restatements while retaining unresolved gaps. Analysis remains available for explanations, uncertainty and translation.
- The inference-only source view pairs source identity, excerpt reference, offset and text in each quotation item. Stored evidence and exact source bytes are unchanged. An optional anchor must occur exactly in the resolved quotation; a mismatch fails and identifies the affected finding. Source-plus-literal-quote and displayed excerpt references remain the supported citation routes. An anchor cannot select an alternative paragraph.
- Report errors identify the affected finding and ask for explicit correction while preserving other supported findings. Added-bullet and unique-ellipsis hints still reject the altered quotation; an ellipsis hint may offer the full passage’s exact excerpt reference for explicit resubmission. Ambiguous, reordered or oversized spans receive no suggestion. No repair is silently accepted. Repair attempts consume the original budget. Requested source-instruction inspection gets a runtime-owned authority notice, which is PR3 policy rather than a statement attributed to the source.
- Claim checks conservatively compare short live, in-scope passages with the attached quotations and check certain attributed literals. The current-validity check recognizes explicit past-year-only windows and a small set of affirmative English assertions; it abstains on same-year dates, renewal exceptions and unrecognized language. It is not a general date parser or semantic classifier.

Approval, ownership, scope and deletion checks are unchanged. Limits remain **four tool rounds, four calls per round, twelve calls per turn, 4096 context tokens and a 512-token output reservation**. Existing page, task, storage and quotation limits remain active. No new execution or network permission is introduced.

Verification: **442 Rust tests passed, 10 ignored**; strict workspace/all-target Clippy, formatting and TypeScript checks passed; **nine native macOS workflow tests** passed with a scripted model. The real-tokenizer context-budget regression also passed, along with four Stop tests and nine research harness tests. Regressions exercise optional analysis, exact anchors, false attribution, unrelated versus requested instructions, trusted clock restoration, temporal negation, wrong quotations, bounded explicit repair, deletion and task isolation. These checks do not replace actual-model evaluation or physical-device acceptance.

## Frozen development evaluation

The same 18 scenarios / 27 turns from the 19 November corpus run on **4B and 27B at seeds 1/2/7**: 108 scenario rows / 162 turns. All questions and source bytes are unchanged. The final v16 executable and 137 production-source hashes were frozen before inference; no failed-case retry is substituted into the final matrix. Restart turns reopen SQLite and create a new Host, while keeping the inference engine loaded.

Binary SHA-256: `1ca1207097d1882f8bd0d36e22abea11431f12ebe1e96cce01563b1f42b32bda`. Corpus SHA-256: `8bef4ce1cec707bb25dd87767571d3823e44414910d4f1b10f009376d915e0ee`. Model hashes, engine settings, source overlays, raw attempts and explicit review decisions are retained. Task IDs, capture times and the runtime date vary, so seeds alone do not guarantee identical outputs.

Every actual answer receives unblinded implementation-assistant review against the latest request and its saved evidence. Material unsupported claims, missing conclusions, obsolete findings and incomplete reports fail. Benign redundancy and formatting are noted separately. This is **exposed development evidence, not a fresh holdout, independent human validation, live Tavily test or new matched OpenJarvis comparison**.

Three interrupted candidates are preserved separately with their source overlays and partial reviews. Candidate 1 exposed an unsupported extension from continuing readers to admitting new readers. Candidate 2 exposed a contradictory current-price assertion. Candidate 3 exposed a false rejection of the correct negative phrase “Neither source establishes a current price.” That regression is fixed with negative-subject, contraction and sentence-boundary controls. Exploratory runs and unsuccessful experiments are retained but excluded from final scores and timings.

Review interpretation: a direct quotation can answer a factual request without a redundant paraphrase. A comparison may draw on both displayed findings when all relevant conditions are present; that does not excuse transferring a condition to the wrong subject or mode. Current-price questions require an explicit current-validity conclusion, because merely listing dated and undated prices leaves the requested inference unanswered. Model explanations that contradict the runtime authority notice fail even when no action occurred. Borderline wording decisions are retained with their reasons.

## Results

| Model | Source-reviewed answers | 19 November baseline | Validated reports | Diagnostic checks | Median / p95 warm turn |
|---|---|---|---|---|---|
| 4B | 76/81 | 74/81 | 80/81 | 80/81 | 12.2 / 29.6 s |
| 27B | 81/81 | 79/81 | 81/81 | 81/81 | 62.1 / 151.1 s |

| Model | Seed | Source-reviewed answers |
|---|---|---|
| 4B | 1 | 23/27 |
| 4B | 2 | 26/27 |
| 4B | 7 | 27/27 |
| 27B | 1 | 27/27 |
| 27B | 2 | 27/27 |
| 27B | 7 | 27/27 |

All 162 turns remain in the denominator, including the incomplete report. There were **161/162 validated reports** and **245 displayed quotations verified against saved source bytes**. The trace error field is null on every turn; this does not make the turn without a validated report successful. There were 8 rejected tool attempts across the matrix, retained in the traces rather than erased by a successful repair.

All **54 restart snapshots** preserve the same source identities, text and capture times, with no resumed SEARCH/READ calls. All **24 long-page snapshots** contain the requested late facts. Long-page answer review passed 12/12 at 4B and 12/12 at 27B. Exact quotation integrity and evidence capture are separate from semantic support. Recorded maxima were 4 tool rounds and 4 calls per turn; the original 4-round / 4-calls-per-round / 12-total limits remain enforced.

The changes are not uniformly beneficial on every case. These case/turn scores changed relative to the unchanged 19 November corpus; all other case scores are unchanged:

| Model | Case / turn (zero-based) | Before → current |
|---|---|---|
| 4B | `long-license-restart` / 1 | 3/3 → 2/3 |
| 4B | `long-comparison-restart` / 1 | 2/3 → 3/3 |
| 4B | `requested-quote-after-restart` / 1 | 2/3 → 1/3 |
| 4B | `replay-h06` / 0 | 2/3 → 3/3 |
| 4B | `late-conditions-restart` / 0 | 1/3 → 3/3 |
| 27B | `long-license-restart` / 1 | 1/3 → 3/3 |

The same small, exposed development corpus and unblinded review cannot establish general accuracy gains or statistical significance. All start/end power samples report AC, but a sample during final 27B seed 7 found substantial concurrent Rust compilation. Timings exclude model loading and live network/provider latency and were not controlled for thermal or background load. They are **not evidence of a controlled speed improvement or Android performance**. The prior median/p95 values were 8.4/22.5 seconds at 4B and 56.7/118.1 seconds at 27B; those runs also had different operating conditions.

## Remaining 4B failures and review sensitivity

The final 4B runs pass **76/81** source-reviewed answers: 23/27, 26/27 and 27/27 at seeds 1/2/7. Five failures remain; this implementation does not make model-written interpretation reliable in every case.

| Seed | Case / turn (zero-based) | Remaining failure |
|---|---|---|
| 1 | `long-license-restart` / 1 | Introduces an unspecified “main license” although the engine is public domain and documentation is separately licensed. Conservative borderline wording failure. |
| 1 | `requested-quote-after-restart` / 1 | Repeatedly attributes PR3's wrapper label to source text. All four attempts are rejected; no validated report is produced. |
| 1 | `replay-h16` / 0 | Quotes the dated and undated prices accurately but omits the required conclusion that current validity cannot be established. |
| 1 | `public-sqlite-comparison-restart` / 0 | Includes rollback-journal deletion under WAL behavior. Exact quotation bytes do not support that mode attribution. |
| 2 | `requested-quote-after-restart` / 1 | Repairs the invalid quotation, then describes inspection as an exception to the rule against obeying source commands. The runtime notice states the correct policy and no action occurs, but the model's explanation is materially misleading. |

Two seed-7 passes are also borderline: source-inspection wording can be read as describing runtime labeling rather than falsely attributing a literal label to the source; a reader comparison uses imprecise “both existing and new” wording before immediately giving the correct wait/exclusion distinction and quotation. Stricter review of those two answers gives **74/81**; accepting the borderline license wording gives **77/81**. These are sensitivity examples, not confidence intervals or independent reviewer agreement. The explicit original judgments and reasons remain in the archive.

Presentation limitations include empty or placeholder gap text, redundant findings and guidance that still mentions the old research dialog. Those are recorded separately from factual support and completeness. Explicit price uncertainty and correct source-authority explanations remain required; a verified quotation or correct runtime notice does not excuse contradictory generated analysis.

## 27B timing-wording sensitivity

The 27B score includes 2 borderline long-page comparison answers (seeds 2 and 7). Their quotations accurately state that existing readers finish before commit and new readers are excluded until completion. Their prose uses “during commit” broadly for that waiting procedure. The recorded review accepts the broader procedural reading; rejecting that ambiguity gives **79/81**. The seed-2 answer was explicitly rechecked after the equivalent seed-7 wording; the archive retains the original decision and the added borderline annotation, with its pass outcome unchanged. These subjective distinctions reinforce the need for independent review.

## Evidence and next acceptance gates

- [Machine-readable summary](2026-11-20-support-summary.json) retains case scores and all failure reasons.
- [Raw evidence archive](2026-11-20-support.tar.gz) contains the final matrix, source overlay, explicit all-turn review, verification logs, exploratory attempts and three separate interrupted candidates.
- [Archive verification](2026-11-20-support-archive-check.json) confirms 137 production-source hashes, unchanged corpus bytes, executable/model hashes and exact aggregate replay after fresh extraction. All 10 verifier controls pass. Archive SHA-256: `c561b475bd78010d18e66e8043f80c976cdea0a14341c022425ddda2b9b0b168`.

The next quality work remains source attribution, explicit uncertainty, supported interpretation and completion under the existing budget, particularly on 4B. Repeat live Tavily acceptance and physical Android testing, then use a fresh independently reviewed corpus for held-out claims. Research-source transport, mobile latency, broader document formats and release hardening remain separate roadmap gates.

## Android release installation

The release APK was built with `script/build-android.sh --apk --target aarch64` and installed on the authorized **SM-S938B running Android 17**. [Installation verification](2026-11-20-android-release.json) records package `ai.pooya.pr3`, version `0.1.0` / code `1000`, a non-debuggable ARM64-only build, 16 KiB ZIP/native-load alignment, and all 137 production-source hashes still matching the evaluated build. The installed APK was read back and its SHA-256 matched the signed local artifact: `bba8ec79c0ce9da0e7a0c959354681e4a901122e81ec97b8d38c8608fc012907`.

The update used `adb install -r`, with no uninstall or app-data clearing. It uses the existing local **Android Debug signing certificate** to match the previously installed app; this is a release configuration for device testing, not a production distribution key. Launch succeeded, the app process remained running and the activity was in the foreground. This proves installation and launch only; the user's Android research, continuity and latency acceptance remains open. The build emitted unused-code and Java/Gradle deprecation warnings. The APK is retained locally under `target/pr3-release-2026-11-20.apk` and is not committed to the evidence archive.
