# Live Tavily acceptance: macOS, 2026-11-16

**Search works, but general research acceptance fails on this build.** Three public online requests and two resumed turns produced one supported answer and four context-limit failures. This is a diagnostic sample, not a population accuracy estimate. The frozen comparison runs separately and does not use these live pages.

The user saved the Tavily key in the isolated `PR3 Research Check` app. Testing used its native Research dialog, web toggle, task list and source viewer with Qwen3.5-4B-Q4_K_M. No key was read or exported. The [trace-derived summary](2026-11-16-live-summary.json) includes only the five public test turns; the user's preceding personal chats and full page text are excluded. Full traces remain in the isolated app's local storage.

| Request | Runtime result | Turn time | Inference | Tools |
|---|---|---:|---:|---:|
| SQLite WAL versus rollback, official sources requested | Failed: 4,490 prompt tokens exceed the context budget; selected a third-party page | 12.241 s | 8.472 s | 3.769 s |
| SQLite copyright page, status and exceptions | Failed after two saved page parts: 4,106 prompt tokens | 12.843 s | 8.421 s | 4.422 s |
| Same copyright task after restart, web disabled | Saved source recovered; answer failed at 3,821 prompt tokens | 6.467 s | 6.462 s | 0.005 s |
| Short example.com purpose, one finding | Supported answer, exact quote validated and inspectable | 12.325 s | 9.312 s | 3.013 s |
| Same short page after restart, web disabled | Invalid source-ID attempt, then source lookup/read; answer failed at 3,849 prompt tokens | 9.636 s | 9.631 s | 0.005 s |

The model reserves 512 tokens for the answer inside its 4,096-token context, so a prompt smaller than 4,096 can still fail. Report these as total turn times, not tokens/second or first-token latency. The UI buffers final sourced answers; the trace records the first tool-result event separately. Cold application/model startup, repeated-run distributions, memory peaks, power consumption and Android latency were not measured. Live and frozen model workloads did not overlap.

## Results and recovery checks

Tavily completed all three online searches. The resulting source inventories contained five, five and six candidates respectively, including user-provided URLs. For the successful short-page test, search took 2.268 seconds, page reading 0.742 seconds and report validation 0.003 seconds. The first tool-result event occurred 4.047 seconds after turn start; the complete sourced answer took 12.325 seconds. These are observations from one request, not a service-level guarantee.

The successful finding accurately described the [example.com page](https://example.com/), and the report's quotation matched the retrieved snapshot. Its source viewer showed the page URL, capture time and saved passage. After a real app exit and relaunch, that same source viewer still worked with web reading disabled. The subsequent answer failed; source persistence passing does not mean the resumed workflow passed.

Both failed research tasks appeared in To Do with **Needs attention** and the actual runtime error. Neither was displayed as a completed research answer. The restored SQLite and example.com reads used their prior capture timestamps, and their tool calls completed in milliseconds with web reading disabled. This establishes cached-source recovery through the exercised path; packet-level network interception was not performed.

The WAL request selected a third-party page even though the user requested official documentation. The relevant authoritative pages are SQLite's [WAL documentation](https://sqlite.org/wal.html) and [locking reference](https://sqlite.org/lockingv3.html). The copyright request did select the [official page](https://www.sqlite.org/copyright.html), but no answer reached report validation. Neither failed request provides evidence of correct synthesis.

## Next implementation priorities

1. Budget source selection and tool history against the actual tokenizer and reserved answer space. Keep complete immutable snapshots in storage while selecting bounded, relevant excerpts for inference. Preserve provenance and the untrusted-source boundary during compaction.
2. Reuse saved source IDs and passages after restart. Avoid repeating a search inventory or reading an already available passage when it cannot add needed evidence. The short-page resumed turn attempted an unknown ID before recovery.
3. Honor explicit source restrictions such as official-only requests and report when retrieved candidates cannot satisfy them.
4. Add real-page-sized regression fixtures and a clearer context-exhaustion message. Retain approval checks, execution limits and honest task status.

Production code, prompts, context size and budgets were left unchanged for the preregistered comparison. The web preference was restored to enabled, the key retained, and the isolated app quit before benchmark inference began. Temporary plaintext tracing was then disabled in the isolated app's configuration; the captured diagnostics remain local.
