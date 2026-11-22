/* The loopback system under test: answers every query with a fixed token sequence after
 * a set delay, so the harness can be checked without a model (doc/15-benchmarks.md). */
#ifndef PR3_BENCH_SUT_LOOPBACK_H
#define PR3_BENCH_SUT_LOOPBACK_H

#include <stddef.h>
#include <stdint.h>

#include "pr3_loadgen.h"

typedef struct pr3_loopback_options {
    uint32_t tokens;       /* output tokens per sample, at least 1 */
    uint32_t ttft_us;      /* delay before the first token */
    uint32_t tpot_us;      /* delay per further token */
    size_t queue_capacity; /* samples waiting at once; the issue callback waits when full */
} pr3_loopback_options;

typedef struct pr3_loopback pr3_loopback;

/* Starts the worker thread. Returns NULL on bad options or when out of memory. */
pr3_loopback *pr3_loopback_create(const pr3_loopback_options *options);

/* Stops the worker after the queue drains, and frees everything. NULL is ignored. */
void pr3_loopback_destroy(pr3_loopback *loopback);

/* The SUT table for pr3_lg_run; valid until pr3_loopback_destroy. */
pr3_lg_sut pr3_loopback_sut(pr3_loopback *loopback);

/* Samples answered so far. */
uint64_t pr3_loopback_completed(pr3_loopback *loopback);

#endif /* PR3_BENCH_SUT_LOOPBACK_H */
