# Research strict-quality repair — 24 November 2026

**Complete: 240/240 main passes, 236 strict and 240 lenient**, with 4B and 27B at seeds 1/2/7. All 20 strict-review failures from the [2026-11-22 evaluation](2026-11-22-research-coverage-report.md) now pass main and strict review. Four other configurations are new strict failures, each a borderline wording slip in a class the fixes target; no new main failures were found. All 84 restart answers pass main and strict review. The previous scores were 240 main, 220 strict and 240 lenient. Four report attempts were rejected and repaired within the existing budgets; review judges each rejection justified, so the final run has no avoidable repair. These are exposed, unblinded regression results, not independent generalization evidence. The final archive was extracted and replayed successfully; three superseded attempts remain separate.

## Implementation

The 2026-11-22 review left 20 strict-review failures among its 240 main passes. Each change below targets one of their causes and has its own regression tests. Quotation integrity, approvals, website scope and execution budgets are unchanged: **four tool rounds, four calls per round, twelve calls total, 4,096 context tokens and a 512-token answer reservation**.

**Unrequested reference entries (12 Git/Cargo answers).** The coverage plan no longer requires a passage only because its body repeats requested words. A reference entry headed by another option or key (`--relative[=<path>]`, `diff.<driver>.trustExitCode`, `parallel = [...]`) contributes only requested phrases and default/option scope, unless its heading itself matches the request. A sentence that points forward to absent text ("looks like this:") cannot become an excerpt or a primary quotation while a substantive sentence exists. Comparison stems such as "another", "include", "change" and "versus" no longer count as requested facts; the stem list previously compared unstemmed words, so "another" and "versus" were never removed. The condition bonus now needs a requested word or phrase in the same passage. Unplanned passages remain readable by the model; they are no longer mandatory citations. Before inference, replay of the 238 archived fitted views changed only the 12 targeted Git/Cargo plans, and a full fitting replay of 234 archived generations changed only Git and Cargo, to exactly the rubric passages.

**Clarification examples (3 development answers).** When the website scope needs clarification and there are no findings, the report shows only PR3's own clarification notice. Model-written gaps are not displayed, the host asks for empty gaps and no example sites or organizations, and an empty report without that scope problem remains an error.

**Identity overstatements (2 exposed answers).** For identity questions, calling differing profiles "conflicting", "contradictory", "inconsistent" or "incompatible", and turning "does not mention" into "mentions only", now fails the existing identity-relationship check. The existing bounded repair then restricts the conclusion to cautious uncertainty. Negated wording ("are not contradictory") remains valid. Intensifiers such as "clearly", "definitively" and "conclusively" no longer change the result: "do not definitively establish" counts as uncertainty, like "do not establish", and "are clearly different people" counts as a categorical claim. Hedges such as "likely" are kept. This removes an avoidable rejection of a cautious answer observed in the superseded `strict2` run.

**Language and temporal wording (3 Spanish answers).** Model-written conclusions, finding text and gaps must follow a clearly identified request language. The check uses English, Spanish and French function-word profiles plus Persian script. It abstains on short, mixed or technical text and never inspects quotations. When every attached value is expired or undated for a current-value question, the conclusion may not call the values contradictory, inconsistent, conflicting or incompatible. With one expired and one undated value, it may also not call both expired or say that neither is dated. A quantifier after a negated verb ("no incluye ninguna fecha", "n'a aucune date") or directly before the date itself ("neither date nor validity period") describes one value and is not read as "neither source"; the superseded `strict3` run rejected such a correct answer once. When a report cites two or more sources, its conclusion and gaps may not call one of them the only or only available source unless a qualifier follows ("the only source with a date", "la única nota con fecha"). The truth of a qualifier is not checked, so the 2026-11-22 wording "la única fuente de precio específico" would still pass. The conclusion description now says that sources differing in role, place or date are not contradictory, asks for each source's own value or status, and asks for the request's language.

These checks are narrow lexical rules for the observed wording classes. They are not semantic entailment, general language identification or temporal reasoning. A rejection costs one repair attempt within the existing budget.

## Validation and frozen builds

The final build (`strict4`, executable SHA-256 `936277db…`, frozen at 06:41:51 UTC on 24 November from `ee31d51` plus the archived production patch) passes **511 workspace tests** (13 ignored) plus **four native tokenizer/grammar tests**, strict Clippy (`-D warnings`), formatting and `git diff --check`. Regression tests use the archived wording verbatim where possible:

| Cause | Tests |
|---|---|
| Git/Cargo reference entries | `another_reference_entry_is_not_required_for_its_incidental_keywords`, `conditions_lead_ins_and_relation_verbs_do_not_create_required_findings` |
| Clarification examples | The scope-clarification report test now hides model-written example agencies and still fails an empty unsearched report without a scope problem |
| Identity overstatements | `different_profiles_are_not_conflicts_and_absent_mentions_are_not_exclusive`, `intensifiers_neither_create_nor_remove_an_identity_assertion` |
| Language and temporal wording | `prose_must_follow_a_clear_request_language_and_abstains_otherwise`, `expired_and_undated_values_differ_without_being_called_contradictory`, `an_expired_and_an_undated_value_keep_their_own_statuses`, `a_report_with_several_sources_does_not_call_one_the_only_source`, and the report-level `a_report_citing_two_sources_does_not_call_one_the_only_source` |

Before the final run, a temporary test module (removed before freezing) replayed the conclusion-level checks — current validity, identity, sole source and language — on all **84 archived final conclusions and 13 rejected report conclusions** from the 2026-11-22, `strict1`, `strict2` and `strict3` matrices. Rejected attempts are paired with their turn's final quotations, an approximation of their own citations. The checks reject only the nine final conclusions that review had already judged defective: four 2026-11-22 strict failures, the `strict1` main failure and three `strict1` borderlines, and the `strict2` borderline. The fifth conclusion-level 2026-11-22 failure ("la única fuente de precio específico") still passes because the truth of a qualifier is not checked. Of the 13 rejected attempts, 11 stay rejected and the two avoidable ones are accepted. The 2026-11-22 Spanish rejection that its report called conservative stays rejected: its "no son compatibles temporalmente" is the incompatibility overstatement that strict review fails. The replay log is archived.

## Evaluation and review method

The matrices are unchanged: 162 development turns and 78 exposed turns, with the same case and page bytes (corpus SHA-256 `8bef4ce1…` and `84dc1bbf…`), `Qwen3.5-4B-Q4_K_M.gguf` and `Qwen3.8-27B-Q4_K_M.gguf`, seeds 1/2/7. Each matrix was run with a frozen executable and source overlay, as `bench/research/README.md` describes. No failed-case retry substitutes for a final answer. Every actual answer was reviewed against the latest request and its cited evidence by the implementation assistant, unblinded. Material omissions, unsupported claims and incomplete reports fail main review; a borderline pass fails strict review. These are exposed regression cases, not held-out evidence, independent validation or a new matched OpenJarvis comparison.

Answers were grouped only when the displayed answer was byte-identical, after removing internal citation identifiers, to an already reviewed answer for the same case and turn. Such a display carries over the earlier judgment and names its origin, which may be this run, the superseded `strict1` run or the 2026-11-22 run. All wording, quotations and source labels remain in the comparison. Every new display was read and judged separately.

### Superseded attempts

Every attempt is retained and reviewed; none supplies a final answer. Each ran with its own frozen executable and source overlay.

| Run | Matrices | Main | Strict | What it found |
|---|---|---:|---:|---|
| `strict1` (executable `edae637b…`) | Both, 240 turns | 239/240 | 236/240 | All 12 Git/Cargo, 3 clarification, 2 identity and the English-language failures were fixed. One main failure: development 4B seed 2 `replay-h16` called the undated listing expired. Three Spanish strict failures: "ninguno incluye fecha" despite the stated end date, and "temporalmente incompatibles" twice |
| `strict2` (`9117a0b5…`) | Both, 240 turns | 240/240 | 239/240 | One strict failure: exposed 4B seed 7 "La única fuente disponible indica…, mientras que otra nota…". The identity check also rejected a cautious 4B seed 1 answer ("do not definitively establish") once, an avoidable repair |
| `strict3` (`6190326c…`) | Exposed only, 78 turns | 78/78 | 78/78 | The new status rule rejected a correct 4B seed 2 Spanish answer once: "no incluye ninguna fecha" describes only the catalogue. The runner was stopped 4 seconds after the exposed matrix finished, before the development matrix produced a row, and the rule was narrowed for the final run |

The `strict1` matrices were first written as `research-strict-*-2026-11-23` and renamed `research-strict1-*` before `strict2`; their logs keep the original names.

## Results

| Matrix | Model | Main | Strict | Lenient |
|---|---|---:|---:|---:|
| Development | 4B | 81/81 | 80/81 | 81/81 |
| Development | 27B | 81/81 | 81/81 | 81/81 |
| Exposed regression | 4B | 39/39 | 37/39 | 39/39 |
| Exposed regression | 27B | 39/39 | 38/39 | 39/39 |
| Total | Both | **240/240** | **236/240** | **240/240** |
| Restart subset | Both | **84/84** | **84/84** | **84/84** |

Each model scores 120/120 main; 4B scores 117/120 strict and 27B 119/120. Per-seed scores preserve the complete denominators:

| Matrix | Model | Seed | Main | Strict |
|---|---|---:|---:|---:|
| Development | 4B | 1 | 27/27 | 27/27 |
| Development | 4B | 2 | 27/27 | 26/27 |
| Development | 4B | 7 | 27/27 | 27/27 |
| Development | 27B | 1 | 27/27 | 27/27 |
| Development | 27B | 2 | 27/27 | 27/27 |
| Development | 27B | 7 | 27/27 | 27/27 |
| Exposed regression | 4B | 1 | 13/13 | 12/13 |
| Exposed regression | 4B | 2 | 13/13 | 13/13 |
| Exposed regression | 4B | 7 | 13/13 | 12/13 |
| Exposed regression | 27B | 1 | 13/13 | 12/13 |
| Exposed regression | 27B | 2 | 13/13 | 13/13 |
| Exposed regression | 27B | 7 | 13/13 | 13/13 |

The 20 previous strict failures, compared for the same case, turn, model and seed. Turn indices start at zero.

| Previous strict failure (2026-11-22) | Cause | Final main / strict |
|---|---|---|
| Development, 4B seed 1, `official-source-clarification`, turn 0 | Clarification examples | Pass / pass |
| Development, 4B seed 2, `official-source-clarification`, turn 0 | Clarification examples | Pass / pass |
| Development, 27B seed 7, `official-source-clarification`, turn 0 | Clarification examples | Pass / pass |
| Exposed, 4B seed 1, `fresh-git-diff`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 4B seed 1, `fresh-cargo-features`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 4B seed 1, `fresh-identical-name`, turn 0 | Identity overstatement | Pass / pass |
| Exposed, 4B seed 2, `fresh-git-diff`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 4B seed 2, `fresh-cargo-features`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 4B seed 2, `fresh-spanish-dates`, turn 0 | Temporal wording | Pass / pass |
| Exposed, 4B seed 7, `fresh-git-diff`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 4B seed 7, `fresh-cargo-features`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 27B seed 1, `fresh-git-diff`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 27B seed 1, `fresh-cargo-features`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 27B seed 2, `fresh-git-diff`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 27B seed 2, `fresh-cargo-features`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 27B seed 2, `fresh-spanish-dates`, turn 0 | English conclusion and temporal wording | Pass / pass |
| Exposed, 27B seed 7, `fresh-git-diff`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 27B seed 7, `fresh-cargo-features`, turn 0 | Git/Cargo reference entries | Pass / pass |
| Exposed, 27B seed 7, `fresh-identical-name`, turn 0 | Identity overstatement | Pass / pass |
| Exposed, 27B seed 7, `fresh-spanish-dates`, turn 0 | Temporal wording | Pass / pass |

The **four accepted borderline answers** are not full-quality successes. They are new configurations, but each belongs to a class this work targets, in wording the lexical checks do not cover:

- Development, 4B seed 2, `replay-h16`, turn 0: opens by saying the dated offer establishes a price of 31 credits in answer to a current-price question, and treats the expired offer as of unknown validity instead of saying it has ended. It never promotes either value to a current price.
- Exposed, 4B seed 1, `fresh-spanish-dates`, turn 0: calls both notes "información histórica", although the catalogue is undated rather than historical, and barely compares them. The quotations and the Spanish runtime notice state each status correctly.
- Exposed, 4B seed 7, `fresh-identical-name`, turn 0: says each page "explicitly excludes" work in the other field, although the pages only do not mention it. It still says the pages do not establish one person.
- Exposed, 27B seed 1, `fresh-identical-name`, turn 0: says the name, occupation and location differ, although both pages name the same Lee Morgan. It still says the pages establish neither one person nor separate people.

Generation is not deterministic across runs at the same seed, and each run produced new wording. Across the four `strict` runs, the answers that failed strict review fall in three cases: `fresh-spanish-dates` (three in `strict1`, one in `strict2`, one in the final run), development 4B seed 2 `replay-h16` (a main failure in `strict1` and a borderline in the final run) and, in the final run only, `fresh-identical-name`. The remaining classes — absent mentions stated as exclusions, and temporal statuses collapsed or blurred — recur in new wording and are not closed by these rules.

Review found no final answer in a language other than the request's. The consistency check found 59 distinct displays across the final 240 turns. Across the 2026-11-22 run and all four `strict` runs there are 141 distinct displays, 39 of them shared between runs, with consistent judgments for every identical case, turn and display.

## Integrity, limits and reproducibility

All 240 turns ended with a validated report and no runtime error. **372 displayed quotations** verify against their saved sources. The inference audit checks **479 quotations across 474 inference passes**, including exact ranges and source bindings. Maximum fitted prompt size is 3,234 tokens on development and 3,549 on exposed cases, with the full 512-token answer reservation retained. Recorded maxima are four tool rounds and five tool calls in a turn; no configured limit was exceeded.

All **84 restart snapshots remain unchanged**, all 84 focused restart evidence checks pass, and no restart turn performs search/read retrieval. All 36 focused initial coverage checks pass. The 24 long-page case snapshots retain every requested fact, and 42 displayed citations refer to passages beyond part 12.

The plan audit checks 232 generated report attempts containing 374 required citation selections, with **zero omitted planned references**. The 2026-11-22 run required 408. Displayed quotations fell from 408 to 372, all in the two Git/Cargo cases (`fresh-git-diff` 30 → 18, `fresh-cargo-features` 48 → 24); every other case displays as many quotations as before. Twelve development reports use the separate explicit-instruction-quotation or empty-evidence clarification paths. Counts include **four rejected report attempts**, all in exposed cases and all repaired within the existing budgets:

| Configuration | Rejected conclusion | Rule |
|---|---|---|
| 4B seed 1, `fresh-identical-name` | "describe two different people with the same name" | Identity relationship |
| 4B seed 2, `fresh-identical-name` | "The two provided pages describe different people" | Identity relationship |
| 27B seed 1, `fresh-spanish-dates` | "Las dos notas son temporalmente incompatibles" | Current validity |
| 27B seed 2, `fresh-spanish-dates` | "Las dos notas son incompatibles" | Current validity |

Each rejected conclusion makes a claim that review fails, so none is an avoidable repair. The 2026-11-22 final run had three rejected attempts, one of which its report called avoidable; the superseded `strict2` and `strict3` runs had one avoidable rejection each, both fixed before the final run. The number of rejections did not fall: the stricter checks now reject wording that previously reached the user. Each rejection costs one repair attempt and its latency. These are real failed attempts followed by checked reports, not first-attempt successes or substituted retries.

The [raw archive](2026-11-24-research-strict.tar.gz), [recomputed summary](2026-11-24-research-strict-summary.json) and [extraction/replay verification](2026-11-24-research-strict-archive-check.json) preserve the complete final matrices, source reviews, source overlays, validation logs, the conclusion replay log, power samples, comparison and consistency checks, and the separate superseded `strict1`, `strict2` and `strict3` matrices with their own source overlays and replay summaries. The verifiers are the 2026-11-22 archive's scripts, unchanged. All **49 positive and corruption controls** pass. Extracted replay, inference and plan audits reproduce the originals for the final and superseded matrices. The frozen executable, both model files and all 145 production source hashes match their manifests; the previous archive's hash also verifies.

Archive SHA-256: **885fc56c04f95ed2798bc104b78c85c44af3bc58620a895a27e42e1c00f91f8f**. Replay requires Node with TypeScript support, but no model weights or executable:

```sh
review_dir=$(mktemp -d)
tar -xzf bench/research/evidence/2026-11-24-research-strict.tar.gz -C "$review_dir"
node "$review_dir/development/replay.ts"
node "$review_dir/exposed/replay.ts"
node "$review_dir/development/check-replay.ts"
node "$review_dir/exposed/check-replay.ts"
```

## Observed performance and remaining acceptance

The final matrices ran from **06:41:55 to 09:50:54 UTC on 24 November**: exposed first, until 07:48:43, then development. Power switched from AC to battery at 08:52:26 and stayed on battery for the last hour of the development matrix. The 379 power samples have no gap over 60 seconds, and turn and wall clocks do not diverge. The Mac was far from idle: background compiler activity appears in 54 samples, other processes reached 1,263.6% aggregate CPU, one-minute load reached 221.2 and free memory fell to 57 MiB.

| Matrix | Model | Median turn (s) | p95 (s) | Maximum (s) |
|---|---|---:|---:|---:|
| Development | 4B | 12.82 | 22.71 | 25.53 |
| Development | 27B | 68.32 | 128.86 | 158.37 |
| Exposed regression | 4B | 15.90 | 35.65 | 38.19 |
| Exposed regression | 27B | 90.80 | 183.22 | 195.98 |

Every final median is higher than in the 2026-11-22 run. The superseded runs, which carry nearly the same checks, were faster than both under lighter load:

| Run | Power | Maximum one-minute load | Median turn, development 4B / 27B (s) | Median turn, exposed 4B / 27B (s) |
|---|---|---:|---:|---:|
| 2026-11-22 | AC | 15.4 | 9.62 / 56.41 | 10.89 / 68.72 |
| `strict1` | AC | 5.2 | 7.74 / 41.69 | 9.51 / 50.41 |
| `strict2` | AC, battery after 56 minutes | 12.4 | 7.64 / 44.62 | 9.49 / 51.21 |
| `strict3` | Battery, then AC | 122.1 | – | 12.52 / 99.57 |
| Final (`strict4`) | AC, battery for the last hour | 221.2 | 12.82 / 68.32 | 15.90 / 90.80 |

Timing follows background load more closely than the build. Load, power and generated answers all differ between runs, so this is observational and supports no speed claim in either direction. The checks are lexical and run once per report attempt; repairs, which add a model generation, are counted in the timings.

These runs use the production Host with offline fixture pages. Timings exclude model loading and include repairs. Restart cases reopen storage and create another Host while retaining the loaded model.

Still open, outside this offline work:

- **Live Tavily, native UI and Android acceptance** on this build. The earlier online-repair APK remains the last documented Android installation; no mobile latency claim is made.
- **Latency under controlled power and load**, to separate any cost of the checks from background activity.
- **An independently reviewed, frozen held-out corpus**, before any fresh generalization or matched OpenJarvis claim. This evaluation reuses exposed cases and an unblinded reviewer.
- **The recurring wording classes.** Absent mentions stated as exclusions, and blurred temporal statuses, recur in new wording at the same seeds. Narrow lexical rules catch the observed variants only; the truth of a qualifier such as "la única fuente de precio específico" is not checked. Exact quotation integrity and a complete coverage plan do not establish general semantic understanding.
