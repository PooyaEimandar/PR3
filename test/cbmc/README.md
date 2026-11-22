# Bounded model checking

CBMC harnesses for the base library's containers and decoders and for the JSON number
path (doc/03, "Verification";
doc/14, "Model checking"). Each harness drives one primitive through every sequence of
operations up to a small bound, with nondeterministic sizes and inputs, and asserts the
properties the unit tests sample: results stay inside their block, live allocations never
overlap, accounting is exact, stale handles never resolve, decoding agrees with the
well-formed byte sequences of the Unicode Standard, and the number scanner stays inside its
input and keeps integers exact.

    tools/run-cbmc.sh            run every harness
    tools/run-cbmc.sh arena      run one

The bounds are in each harness and in `tools/run-cbmc.sh`, per loop where one loop needs
far more passes than the rest; CBMC's unwinding assertions fail the run if a bound is too
small to cover the loop it unrolls.

The JSON number harness leaves out inputs whose exponent is between 10 and 399, which reach
the exact 800-digit decimal; the run bounds the decimal's loops at zero, so any input it
keeps that reaches them fails. The vectors, the random round trips and `fuzz_json` cover
that path.

CBMC has no model of Clang's C11 atomic builtins, so the ring harness builds the ring in
its fenced-word mode (`-DPR3_RING_FENCED_WORDS=1`, the mode boards without atomics use).
The index arithmetic is the same in both modes; ordering between threads is checked by
ThreadSanitizer in `test_ring_transfers_across_threads`.
