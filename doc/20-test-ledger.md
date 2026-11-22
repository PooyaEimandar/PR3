# Test ledger

Every behaviour PR3 promises, the test that pins it, the issue that owns it, and where it stands. A milestone closes only when its rows pass; a release requires every row of its profiles passing. SPEC-02 completes this ledger and REL-02 closes it.

**Status:** *open* (not written), *written* (the test exists and runs), *passing* (the test passes under `asan`, `ubsan` and `release`, and the module's golden vectors replay byte for byte). Tests are functions `test_<behaviour>` in `test/unit/test_<module>.c` ([part 13](13-project-structure.md#rules), rule 5), so this ledger can be checked by grep with `tools/check-ledger.sh`.

## Summary

| Area | Tests in | Planned tests | Owner | Status |
| --- | --- | --- | --- | --- |
| Base library | `test_base.c`, `test/vectors/base/` | 41 | FND-03 | Passing |
| Canonical JSON | `test_json.c`, `test/vectors/json/` | 24 | CORE-01 | Passing |
| Agent runtime | `test_core.c`, `test/vectors/core/` | 51 | CORE-02 | Passing |
| Trusted side | `test_trusted.c` | 21 | CORE-03 | Listed below |
| Wire protocol | `test_proto.c` | 2 | CORE-04 | Open |
| Memory model | `test_memory.c` | 7 | DATA-03 | Open |
| Inference host | `test_infer.c` | 48 | INF-01, INF-02 | Open |
| Inference session | `test_infer_session.c`, `test/model/` | 26 | INF-03 | Open |
| Evidence and report grammar | `test_evidence.c` | 31 | DATA-05 | Open |
| Encrypted stores | `test_store.c` | 52 | DATA-02, DATA-04, DATA-05, DATA-09 | Open |
| Tools | `test_tool.c` | 83 | DATA-06, DATA-05 | Open |
| Hybrid | `test_hybrid.c` | 9 | DATA-08 | Open |
| Vault and speech | `test_platform.c` | 8 | PLT-01, PLT-02 | Open |
| App host | `test_app_host.c` | 175 | APP-03, APP-06, DATA-03, DATA-07 | Open |
| Host commands | 62 commands ([part 10](10-app-sdl3.md#host-commands-and-events)) | One or more each | APP-03, APP-06 | Open |
| Benchmark harnesses | `bench/mlperf/`, `bench/agentdojo/` | Smoke runs, suites | BEN-01, DATA-10 | Open |
| Fuzz targets | `test/fuzz/fuzz_json.c`, `test/fuzz/fuzz_core_step.c` | 2 | CORE-01, CORE-02 | Passing |
| Fuzz targets, to come | `test/fuzz/fuzz_{memory_search,model_output,protocol_frame,trusted_gate,web_page}.c` | 5 | DATA-03, INF-02, CORE-04, CORE-03, DATA-05 | Open |

## Base library (FND-03)

Passing on the development Mac under `dev`, `asan`, `ubsan`, `tsan` and `release`, and on the Android emulator, 21 Nov 2026.

| Test | Status |
| --- | --- |
| `test_every_status_has_text` | Passing |
| `test_arena_allocations_are_aligned_and_disjoint` | Passing |
| `test_a_full_arena_returns_null_and_stays_usable` | Passing |
| `test_arena_size_overflow_is_refused` | Passing |
| `test_arena_rewind_releases_only_what_came_after_the_mark` | Passing |
| `test_an_empty_arena_hands_out_only_zero_bytes` | Passing |
| `test_released_arena_memory_is_poisoned` | Passing |
| `test_pool_handles_resolve_until_released` | Passing |
| `test_a_stale_handle_resolves_to_null_and_cannot_release` | Passing |
| `test_a_full_pool_refuses_and_recovers_after_a_release` | Passing |
| `test_acquired_slots_are_zeroed` | Passing |
| `test_a_slot_whose_generation_would_wrap_is_retired` | Passing |
| `test_pool_init_leaves_a_small_arena_unchanged` | Passing |
| `test_pool_bytes_covers_what_init_takes` | Passing |
| `test_buddy_blocks_are_powers_of_two_and_aligned` | Passing |
| `test_buddy_coalesces_back_to_one_block` | Passing |
| `test_a_full_buddy_heap_returns_null` | Passing |
| `test_buddy_random_traffic_never_overlaps_and_accounts_exactly` | Passing |
| `test_buddy_bytes_covers_what_init_takes` | Passing |
| `test_buddy_rejects_sizes_that_are_not_powers_of_two` | Passing |
| `test_ring_is_fifo_and_reports_full_and_empty` | Passing |
| `test_ring_indices_wrap` | Passing |
| `test_ring_rejects_bad_capacity` | Passing |
| `test_ring_transfers_across_threads` | Passing |
| `test_builder_overflow_is_sticky_and_reported_once` | Passing |
| `test_builder_writes_integers` | Passing |
| `test_trailing_zero_bits_are_counted` | Passing |
| `test_builder_writes_lowercase_hex` | Passing |
| `test_clip_cuts_on_a_character_boundary_with_the_marker` | Passing |
| `test_str_compare_is_byte_order` | Passing |
| `test_parse_u64_accepts_only_canonical_decimal` | Passing |
| `test_parse_u64_refuses_overflow` | Passing |
| `test_utf8_accepts_every_well_formed_sequence` | Passing |
| `test_utf8_rejects_overlongs_surrogates_and_out_of_range` | Passing |
| `test_utf8_rejects_truncated_sequences` | Passing |
| `test_utf8_encode_round_trips_every_scalar_value` | Passing |
| `test_utf8_validation_agrees_with_decoding_on_random_bytes` | Passing |
| `test_str_from_bytes_refuses_invalid_utf8` | Passing |
| `test_siphash24_matches_the_papers_example` | Passing |
| `test_siphash_matches_the_golden_vectors` | Passing |
| `test_vec_push_stops_at_capacity` | Passing |

## Canonical JSON (CORE-01)

Passing on the development Mac under `dev`, `asan`, `ubsan`, `tsan` and `release`, and on the Android emulator, 22 Nov 2026. The fuzz target `test/fuzz/fuzz_json.c` checks the same properties on arbitrary input ([part 14](14-build-and-quality.md#fuzzing)); its first hour ran 50 549 773 inputs with no failure (22 Nov 2026).

| Test | Status |
| --- | --- |
| `test_canonical_vectors_match` | Passing |
| `test_number_vectors_match` | Passing |
| `test_invalid_vectors_are_refused` | Passing |
| `test_nesting_up_to_the_limit_is_accepted` | Passing |
| `test_nesting_beyond_the_limit_is_refused` | Passing |
| `test_the_tape_records_kinds_counts_and_extents` | Passing |
| `test_strings_without_escapes_point_into_the_input` | Passing |
| `test_duplicate_keys_keep_the_last_value` | Passing |
| `test_large_objects_sort_stably_and_keep_the_last_duplicate` | Passing |
| `test_errors_say_where_and_why` | Passing |
| `test_a_small_arena_fails_cleanly` | Passing |
| `test_empty_and_null_inputs_are_refused` | Passing |
| `test_random_inputs_stay_within_the_bounds` | Passing |
| `test_long_numbers_scale_by_their_whole_exponent` | Passing |
| `test_a_small_buffer_is_reported` | Passing |
| `test_strings_escape_exactly_the_canonical_set` | Passing |
| `test_escapes_are_exact_at_every_offset_and_capacity` | Passing |
| `test_doubles_use_the_canonical_layout` | Passing |
| `test_infinities_and_nans_are_refused` | Passing |
| `test_random_doubles_round_trip` | Passing |
| `test_write_needs_no_more_than_its_bound` | Passing |
| `test_type_names_parse` | Passing |
| `test_an_integer_is_a_number_that_stayed_an_integer` | Passing |
| `test_names_are_short_lowercase_words` | Passing |

## Agent runtime (CORE-02)

Passing on the development Mac under `dev`, `asan`, `ubsan`, `tsan` and `release`, 22 Nov 2026; the library also builds for the `armv6m` and `rv32imc` boards. The vector runner replays `test/vectors/core/steps.jsonl` (61 scenarios, 2,378 calls) byte for byte. The three tests of the core's limits are under [Limits](#limits). The fuzz target `test/fuzz/fuzz_core_step.c` checks the core's invariants on arbitrary scripts ([part 04](04-core-runtime.md#tests)); ten minutes on the final code ran 853 842 inputs with no failure (22 Nov 2026).

| Test | Status |
| --- | --- |
| `test_a_forgotten_session_keeps_nothing_and_others_are_untouched` | Passing |
| `test_a_restored_session_gets_its_exchanges_back_without_tool_traffic` | Passing |
| `test_a_restored_session_keeps_within_the_history_limit` | Passing |
| `test_plain_turn_replies_and_traces` | Passing |
| `test_tool_turn_runs_the_call_then_answers` | Passing |
| `test_independent_reads_share_one_round_and_wait_for_every_result` | Passing |
| `test_batching_cannot_bypass_the_total_call_budget_or_partially_execute` | Passing |
| `test_consequential_calls_are_proposed_never_called` | Passing |
| `test_refused_calls_are_explained_to_the_model` | Passing |
| `test_repeated_identical_call_stops_the_turn` | Passing |
| `test_exhausted_budget_counts_batches_and_refusals_without_entering_history` | Passing |
| `test_final_round_results_allow_an_answer_but_no_further_read_or_approval` | Passing |
| `test_tool_rounds_are_bounded` | Passing |
| `test_one_turn_at_a_time_per_session` | Passing |
| `test_unknown_or_repeated_answers_are_rejected` | Passing |
| `test_model_failure_fails_the_turn_with_a_trace` | Passing |
| `test_oversized_and_empty_input_are_rejected` | Passing |
| `test_history_is_bounded` | Passing |
| `test_history_stays_in_order_across_many_trims` | Passing |
| `test_trimming_never_separates_a_tool_result_from_its_call` | Passing |
| `test_calls_over_the_limit_get_one_refusal_and_refused_arguments_are_clipped` | Passing |
| `test_abort_ends_the_turn_so_the_session_can_continue` | Passing |
| `test_an_abort_answers_every_open_call` | Passing |
| `test_host_notes_reach_the_model_but_not_the_trace_query` | Passing |
| `test_a_brief_stays_on_the_first_message_and_out_of_the_history` | Passing |
| `test_the_upcoming_prompt_is_the_next_turns_before_its_message` | Passing |
| `test_a_new_sessions_first_message_begins_with_its_opening` | Passing |
| `test_a_note_cannot_break_out_of_its_line` | Passing |
| `test_notes_beyond_the_limit_are_left_out` | Passing |
| `test_long_tool_output_is_truncated_on_a_char_boundary` | Passing |
| `test_a_turn_that_read_a_page_does_not_act_and_the_next_one_may` | Passing |
| `test_a_call_beside_a_read_is_refused_too` | Passing |
| `test_failed_untrusted_reads_also_block_actions` | Passing |
| `test_untrusted_calls_leave_with_their_results` | Passing |
| `test_source_metadata_requires_registered_untrusted_output_and_keeps_existing_bounds` | Passing |
| `test_sources_are_deduplicated_and_scoped_to_the_session` | Passing |
| `test_valid_call_is_canonicalized` | Passing |
| `test_untrusted_calls_are_refused_with_a_reason` | Passing |
| `test_confirmation_tools_are_validated_like_any_other_and_marked` | Passing |
| `test_bad_specs_are_rejected` | Passing |
| `test_unchecked_constraints_are_refused_and_enums_are_checked` | Passing |
| `test_identical_call_is_caught` | Passing |
| `test_alternating_tools_with_new_arguments_pass` | Passing |
| `test_budget_is_enforced` | Passing |
| `test_tool_outcomes_without_sources_are_accepted` | Passing |
| `test_core_vectors_match` | Passing |
| `test_random_steps_keep_the_core_consistent` | Passing |
| `test_the_worst_case_fits_the_reported_size` | Passing |

## Trusted side (CORE-03)

| Test | Status |
| --- | --- |
| `test_a_resumed_chain_continues_where_it_stopped` | Open |
| `test_any_rewrite_breaks_the_chain` | Open |
| `test_approved_mail_is_issued_redeemed_once_and_fully_audited` | Open |
| `test_declined_refused_and_denied_requests_issue_nothing` | Open |
| `test_approval_needs_an_approval_and_the_right_key` | Open |
| `test_the_daily_limit_survives_a_restart` | Open |
| `test_a_broken_log_is_refused` | Open |
| `test_requests_left_open_when_the_app_closed_expire_when_it_opens` | Open |
| `test_tampered_receipts_never_issue` | Open |
| `test_late_approvals_and_capabilities_never_authorize` | Open |
| `test_altered_capabilities_are_refused` | Open |
| `test_hex_round_trips_and_rejects_junk` | Open |
| `test_parts_cannot_run_together` | Open |
| `test_names_resolve_to_the_directorys_addresses` | Open |
| `test_the_model_cannot_supply_addresses_identity_or_extra_fields` | Open |
| `test_nothing_hidden_reaches_the_confirmation_screen` | Open |
| `test_text_is_bounded_single_line_where_needed_and_never_reordered` | Open |
| `test_envelopes_are_fresh_and_used_once` | Open |
| `test_unknown_operations_are_refused` | Open |
| `test_every_email_needs_approval_and_risk_grows_with_recipients` | Open |
| `test_kill_switch_daily_limit_and_one_question_at_a_time` | Open |

## Libraries, by area

| Area | Planned tests | Owner |
| --- | --- | --- |
| Wire protocol messages | 2 | CORE-04 |
| Memory types, retrieval, evidence references | 1, 4, 2 | DATA-03 |
| Prompt formats | 22 | INF-01 |
| Text pipeline, tool-call dispatch, Hermes JSON parser, XML parser | 12, 2, 4, 8 | INF-02 |
| KV plan, session, backend | 6, 4, 11 | INF-03 |
| Installed-model tests: `kv_reuse`, `qwen_tools`, `answer_injection` | 2, 2, 1 | INF-03 |
| Evidence fitting, report grammar | 28, 3 | DATA-05 |
| Stores, encryption | 19, 6 | DATA-02 |
| Task store | 9 | DATA-03 |
| Document store | 3 | DATA-04 |
| Research store | 4 | DATA-05 |
| Continuity store | 11 | DATA-09 |
| Tool registry, calculator, clock, documents, memory, task context | 2, 2, 2, 1, 11, 6 | DATA-06 |
| Web reader, research, research sources, research scope | 18, 22, 15, 4 | DATA-05 |
| Hybrid: pairing, channel, node, client | 9 | DATA-08 |
| Vault (approval key) | 3 | PLT-02 |
| Speech | 5 | PLT-01 |

## App host, by area

| Area | Planned tests | Owner |
| --- | --- | --- |
| Chat host, chat flows, backend glue | 30, 6, 6 | APP-03 |
| Tasks: lifecycle, export, results, sources, words | 39, 2, 1, 5, 6 | DATA-03, APP-06 |
| Documents, sources | 3, 3 | DATA-04 |
| Research context, research flows, web reading, search providers | 11, 10, 5, 5 | DATA-05 |
| Memory, memory commands, semantic memory | 7, 1, 8 | DATA-03 |
| Configuration, models, storage, trace export | 7, 11, 3, 2 | DATA-07 |
| Continuity, hybrid, share | 2, 1, 1 | DATA-09, DATA-08, PLT-01 |

## Limits

Every bound PR3 enforces, with the test that pins its edge.

| Behaviour | Limit | Decision | Test |
| --- | --- | --- | --- |
| Sessions held at once | `max_sessions` (64); the least recently used idle session is evicted and restored by the host | [D-04](19-decisions-and-risks.md) | `test_evicted_sessions_are_restored_by_the_host` |
| Turns in flight | `max_turns` (4); a full pool fails the utterance with `PR3_E_NOMEM` | [D-04](19-decisions-and-risks.md) | `test_a_full_turn_pool_refuses_without_changing_anything` |
| Session histories | One 8 MiB buddy heap; a message that cannot fit fails the step with `PR3_E_NOMEM` | [D-04](19-decisions-and-risks.md) | `test_a_full_history_heap_refuses_without_changing_anything` |
| Wire messages | 1 MiB ceiling, separate from each endpoint's own cap | CORE-04 | `test_oversized_frames_are_refused_before_decoding` |
| Accessory frames | 256-byte payload | EMB-01 | `test_accessory_frames_over_the_cap_are_refused` |
| JSON nesting | 127 levels | CORE-01 | `test_nesting_beyond_the_limit_is_refused` |

## Golden vectors

| Format | Directory | Owner |
| --- | --- | --- |
| SipHash-1-3 and SipHash-2-4 (`siphash.jsonl`) | `test/vectors/base/` | FND-03 |
| Canonical JSON: canonical forms, numbers, refused inputs (`canonical.jsonl`, `numbers.jsonl`, `invalid.jsonl`) | `test/vectors/json/` | CORE-01 |
| Core step sequences, including stale events, capacity exhaustion and cancellation (`steps.jsonl`) | `test/vectors/core/` | CORE-02 |
| Trusted hashes, receipts, audit entries | `test/vectors/trusted/` | CORE-03 |
| CBOR wire frames | `test/vectors/proto/` | CORE-04 |
| Prompt renders, Qwen3 and Qwen3.5 | `test/vectors/infer/` | INF-01 |
| Unicode character classes and case mappings | `test/vectors/unicode/` | CORE-03 |

---

[Back to the index](../README.md)
