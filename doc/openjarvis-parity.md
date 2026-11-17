# OpenJarvis capability and acceptance checklist

Reviewed 17 November 2026. This is the implementation checklist for the [roadmap](roadmap.md), not a claim that the products have reached parity. Upstream documentation describes supported features; PR3 acceptance requires working code and measured workflows on the current build. Existing benchmark archives remain historical evidence.

## Finish research first

| Gate | PR3 implementation | Evidence still needed |
|---|---|---|
| Reliable bounded research | Token-budgeted saved evidence; exact quotations; restart recovery; compact task-bound source references; runtime-resolved excerpt quotations; evidence from distinct relevant pages; final round reserved for reporting | [138-turn development replay](../bench/research/evidence/2026-11-17-reliability-report.md) complete, with 58/69 source-reviewed answers at 4B and 65/69 at 27B. Real-page comparison and temporal uncertainty still fail; live/Android acceptance and independent review remain |
| Unified chat and source choices | Natural research requests in the same chat/task; inline Web and Sources controls; ordinary tools return on later turns; candidate, redirect, snapshot, inference and citation checks; corrections can narrow the scope | Physical Android acceptance, broader routing/language evaluation, live redirect edge cases and broader provider coverage. “Official” is not inferred from a page title; the user selects the relevant websites |
| Useful document ingestion | Text/Markdown snapshots and FTS search | Bounded PDF extraction next; DOCX/OCR, folder indexing, incremental updates and document versions afterward |
| Retrieval quality | Exact local full-text retrieval and bounded source excerpts | Semantic/multilingual retrieval and larger-library tests; compare native retrieval with upstream, not only the shared tool adapter |
| Research output | Inspectable saved quotations and model-written findings, with gaps | One approved local export, such as a sourced note, with a verified write result; exact quotations alone do not prove a conclusion |
| Continuity | Local restart, pairing, task/memory ownership and deletion transport | Physical Android↔computer acceptance, source transport, interrupted transfer, deletion convergence and replacement-device recovery |

Enable **Web** in the chat composer, then open **Sources** and use **Only these websites** for 1–4 comma-separated domains, for example `sqlite.org, docs.example.org`. Subdomains are included; local documents and other websites are excluded. The equivalent typed request ends with `Sources only: sqlite.org`. Later user messages or field choices can narrow this list; clearing the field does not broaden it, and a fresh task is required to broaden it. Scope comes from actual user messages and controls, never source text. Ambiguous English official-only requests ask for websites instead of treating search-result labels as proof. This is an explicit domain restriction, not general multilingual authority classification. Web starts off after restart or task switching; saved evidence remains available.

Inference uses short `s:` source references and optional opaque `q:` excerpt references. Each excerpt reference binds the range, exact text and source identity through a checksum; guessed ranges and references copied to another source fail. A citation can supply the excerpt reference alone: the report tool resolves it only within this task's bounded, live, permitted evidence. An explicitly supplied source must match; there is no fallback for a mismatched pair. The tool verifies UTF-8 boundaries and quotation limits, and emits the full canonical provenance. These references grant no access and cannot recover deleted sources. A bounded report-quality guard rejects recognizable unrelated commands addressed to the assistant; it is not a general semantic relevance or entailment checker. Existing approval checks and the four-round/twelve-call limit remain active.

Acceptance sequence: deterministic regressions → 4B/27B development replays → live macOS Tavily workflow with source inspection and restart → user-run Android checklist → controlled-power timing and a fresh, independently reviewed holdout. Preserve the exposed v1 corpus and results as regression history. The earlier matched comparison measures orchestration over common tools, not native search or complete product parity.

## Broader gaps after research acceptance

These follow research; they are not prerequisites to calling a single document workflow useful.

| Upstream capability | PR3 gap | Completion gate |
|---|---|---|
| Scheduled and continuous agents | Durable scheduler, background execution, bounded retries and notifications | A user-created monitor survives restart, reports only verified changes, stops on request and never gains action permission from a schedule |
| Skills and tool composition | Versioned, permission-scoped skill catalog and execution | A reusable research skill uses the existing core, preserves source trust and execution limits, and supports revocation; imported scripts do not run automatically |
| Code assistant | Sandboxed code/file tools | Explicit workspace selection, isolation, resource limits, review of consequential changes and recovery; unrestricted shell access is not a parity shortcut |
| Engine breadth and routing | Additional backend interfaces, device-specific capability selection | Matched quality/latency/energy measurements establish when routing helps; private content never silently goes to a cloud provider |
| Learning from traces | Opt-in trace evaluation, skill/policy optimization and rollback | A frozen evaluation shows a gain without weakening permissions or privacy; failed updates roll back |
| Distribution and operations | Release hardening and some platform key custody | Signed releases, recovery testing, refreshed security matrix, independent review and a completed usefulness trial |

Email/contacts remain retired and the unified social inbox remains deferred by the product decision. Upstream connector breadth is recorded as a difference, not authorization to restore those features. Mobile source continuity is a PR3 acceptance goal; upstream's own roadmap still lists federated memory as active work.

## Upstream sources

- [README and built-in agents](https://github.com/open-jarvis/OpenJarvis/blob/main/README.md): research, code assistant, scheduled/continuous agents and skills.
- [Deep Research guide](https://github.com/open-jarvis/OpenJarvis/blob/main/docs/user-guide/deep-research.md): directory/PDF ingestion, multiple retrieval backends, cited multi-hop research and web fallback.
- [Skills guide](https://github.com/open-jarvis/OpenJarvis/blob/main/docs/user-guide/skills.md): discovery, composition, capability declarations and trace-based optimization.
- [Upstream roadmap](https://github.com/open-jarvis/OpenJarvis/blob/main/docs/development/roadmap.md): current focus and remaining work, including routing, operator hardening and federated memory. Roadmap items are not counted as fully shipped capabilities.
