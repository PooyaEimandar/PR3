/* A C interface to MLCommons LoadGen (doc/15-benchmarks.md#the-harness).
 *
 * LoadGen is C++; this header is plain C so every system under test stays in C.
 * A system under test (SUT) and a query sample library (QSL) are tables of callbacks.
 * LoadGen calls them from its own threads; the SUT answers by calling
 * pr3_lg_first_token() and pr3_lg_complete() from any thread.
 */
#ifndef PR3_BENCH_LOADGEN_H
#define PR3_BENCH_LOADGEN_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* One query sample: `id` identifies the response, `index` the sample in the QSL. */
typedef struct pr3_lg_sample {
    uintptr_t id;
    size_t index;
} pr3_lg_sample;

/* One response. `data` is copied by LoadGen before pr3_lg_complete returns, and only
 * in accuracy mode; it may point at the caller's stack. `n_tokens` is the number of
 * output tokens, required by benchmarks that record token latencies. */
typedef struct pr3_lg_response {
    uintptr_t id;
    const void *data;
    size_t size;
    int64_t n_tokens;
} pr3_lg_response;

typedef struct pr3_lg_sut {
    const char *name;
    void *ctx;
    /* Called from LoadGen's issue thread; must not block for long. */
    void (*issue)(void *ctx, const pr3_lg_sample *samples, size_t count);
    /* Called when LoadGen has issued its last query of a run. */
    void (*flush)(void *ctx);
} pr3_lg_sut;

typedef struct pr3_lg_qsl {
    const char *name;
    void *ctx;
    size_t total_count;       /* samples in the dataset */
    size_t performance_count; /* samples that fit in memory at once */
    void (*load)(void *ctx, const size_t *indices, size_t count);
    void (*unload)(void *ctx, const size_t *indices, size_t count);
} pr3_lg_qsl;

typedef enum pr3_lg_scenario {
    PR3_LG_SINGLE_STREAM,
    PR3_LG_MULTI_STREAM,
    PR3_LG_SERVER,
    PR3_LG_OFFLINE
} pr3_lg_scenario;

typedef enum pr3_lg_mode {
    PR3_LG_PERFORMANCE,
    PR3_LG_ACCURACY,
    PR3_LG_SUBMISSION /* accuracy, then performance */
} pr3_lg_mode;

typedef struct pr3_lg_config {
    const char *model; /* the benchmark's key in mlperf.conf, e.g. "llama3_1-8b-edge" */
    pr3_lg_scenario scenario;
    pr3_lg_mode mode;
    const char *user_conf;  /* NULL: the embedded mlperf.conf alone */
    const char *audit_conf; /* NULL: LoadGen's default, "audit.config" in the working directory */
    const char *out_dir;    /* must exist */
    bool summary_to_stdout;
} pr3_lg_config;

/* Status of pr3_lg_run. */
typedef enum pr3_lg_status {
    PR3_LG_OK = 0,
    PR3_LG_BAD_ARGUMENT = 1,
    PR3_LG_BAD_CONFIG = 2,
    PR3_LG_FAILED = 3
} pr3_lg_status;

/* Runs one LoadGen test to completion and writes its logs to cfg->out_dir.
 * Not reentrant: one test per process at a time. */
pr3_lg_status pr3_lg_run(const pr3_lg_sut *sut, const pr3_lg_qsl *qsl, const pr3_lg_config *cfg);

/* Report the first token of each response (token-latency benchmarks only). */
void pr3_lg_first_token(const pr3_lg_response *responses, size_t count);

/* Report completed responses. Each sample is completed exactly once. */
void pr3_lg_complete(const pr3_lg_response *responses, size_t count);

/* LoadGen's version ("6.0.17") and git revision, as compiled in. Static strings. */
const char *pr3_lg_version(void);
const char *pr3_lg_git_revision(void);

const char *pr3_lg_scenario_name(pr3_lg_scenario scenario);

#ifdef __cplusplus
}
#endif

#endif /* PR3_BENCH_LOADGEN_H */
