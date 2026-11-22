/* PR3's microbenchmark harness (doc/15-benchmarks.md, "Module budgets" and "Evidence").
 * Each workload warms up, is calibrated so one run lasts at least --min-ms, then runs
 * --runs times; the median of the runs is the result. Every run is one sample line in
 * the JSON Lines file given with --out. Nothing here ships. */
#ifndef PR3_BENCH_H
#define PR3_BENCH_H

#if defined(__APPLE__)
#define _DARWIN_C_SOURCE 1 /* sysctlbyname's headers need the BSD types */
#elif !defined(_WIN32) && !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L /* clock_gettime, uname */
#endif

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#include <windows.h>
#else
#include <sys/utsname.h>
#include <time.h>
#endif
#if defined(__APPLE__)
#include <sys/sysctl.h>
#endif
#if defined(__ANDROID__)
#include <sys/system_properties.h>
#endif

#ifndef PR3_BENCH_CONFIG
#define PR3_BENCH_CONFIG "unknown"
#endif
#ifndef PR3_BENCH_WORKLOADS
#define PR3_BENCH_WORKLOADS "bench/c/workloads"
#endif

#if defined(__clang__)
#define PR3_BENCH_COMPILER "clang " __clang_version__
#elif defined(__GNUC__)
#define PR3_BENCH_COMPILER "gcc " __VERSION__
#elif defined(_MSC_VER)
#define PR3_BENCH_STR2(x) #x
#define PR3_BENCH_STR(x) PR3_BENCH_STR2(x)
#define PR3_BENCH_COMPILER "msvc " PR3_BENCH_STR(_MSC_FULL_VER)
#else
#define PR3_BENCH_COMPILER "unknown"
#endif

enum { PR3_BENCH_MAX_RUNS = 1000 };

typedef struct pr3_bench {
    const char *module;
    const char *filter;
    const char *workloads;
    char device[256];
    FILE *out;
    unsigned runs;
    double min_ms;
    unsigned failures;
    volatile uint64_t sink; /* keeps every result alive */
} pr3_bench;

/* One call of the workload; returns a value to sink, 0 on failure. */
typedef uint64_t (*pr3_bench_fn)(void *ctx);

static inline uint64_t pr3_bench_now_ns(void) {
#if defined(_WIN32)
    static LARGE_INTEGER freq;
    LARGE_INTEGER now;
    if (freq.QuadPart == 0) QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&now);
    return (uint64_t)((double)now.QuadPart * 1e9 / (double)freq.QuadPart);
#else
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * UINT64_C(1000000000) + (uint64_t)ts.tv_nsec;
#endif
}

/* Writes s as a JSON string; the strings here are ASCII or valid UTF-8. */
static inline void pr3_bench_put_json(FILE *f, const char *s) {
    (void)fputc('"', f);
    for (; *s; s++) {
        unsigned char c = (unsigned char)*s;
        if (c == '"' || c == '\\')
            (void)fprintf(f, "\\%c", c);
        else if (c < 0x20)
            (void)fprintf(f, "\\u%04x", c);
        else
            (void)fputc(c, f);
    }
    (void)fputc('"', f);
}

static inline void pr3_bench_describe_device(char *out, size_t cap) {
    out[0] = '\0';
#if defined(__APPLE__)
    char brand[128] = "", model[64] = "";
    size_t n = sizeof brand;
    if (sysctlbyname("machdep.cpu.brand_string", brand, &n, NULL, 0) != 0) brand[0] = '\0';
    n = sizeof model;
    if (sysctlbyname("hw.model", model, &n, NULL, 0) != 0) model[0] = '\0';
    struct utsname u;
    if (uname(&u) == 0)
        (void)snprintf(out, cap, "%s (%s), %s %s %s", model, brand, u.sysname, u.release, u.machine);
#elif defined(__ANDROID__)
    char model[PROP_VALUE_MAX] = "", maker[PROP_VALUE_MAX] = "", release[PROP_VALUE_MAX] = "",
         soc[PROP_VALUE_MAX] = "";
    __system_property_get("ro.product.manufacturer", maker);
    __system_property_get("ro.product.model", model);
    __system_property_get("ro.build.version.release", release);
    __system_property_get("ro.soc.model", soc);
    (void)snprintf(out, cap, "%s %s (%s), Android %s", maker, model, soc, release);
#elif defined(_WIN32)
    SYSTEM_INFO si;
    GetNativeSystemInfo(&si);
    (void)snprintf(out, cap, "Windows, %lu processors", (unsigned long)si.dwNumberOfProcessors);
#else
    struct utsname u;
    if (uname(&u) == 0) (void)snprintf(out, cap, "%s %s %s", u.sysname, u.release, u.machine);
#endif
    if (out[0] == '\0') (void)snprintf(out, cap, "unknown");
}

static inline int pr3_bench_cmp(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

/* The q-quantile of sorted v by linear interpolation. */
static inline double pr3_bench_quantile(const double *v, unsigned n, double q) {
    if (n == 0 || n > PR3_BENCH_MAX_RUNS) return 0.0;
    if (n == 1) return v[0];
    double pos = q * (double)(n - 1);
    unsigned lo = (unsigned)pos;
    if (lo > n - 2) lo = n - 2;
    return v[lo] + (v[lo + 1] - v[lo]) * (pos - (double)lo);
}

static inline uint64_t pr3_bench_time_calls(pr3_bench *b, pr3_bench_fn fn, void *ctx, uint64_t calls,
                                            bool *ok) {
    uint64_t sink = 0, t0 = pr3_bench_now_ns();
    for (uint64_t i = 0; i < calls; i++) {
        uint64_t r = fn(ctx);
        if (r == 0) *ok = false;
        sink += r;
    }
    uint64_t t1 = pr3_bench_now_ns();
    b->sink += sink;
    return t1 - t0;
}

/* Runs one workload of `bytes` input bytes per call. */
static inline void pr3_bench_run(pr3_bench *b, const char *workload, size_t bytes, pr3_bench_fn fn,
                                 void *ctx) {
    if (b->filter && !strstr(workload, b->filter)) return;
    bool ok = true;
    /* warm up for 50 ms and estimate the cost of a call */
    uint64_t calls = 1, spent = 0;
    while (spent < UINT64_C(50000000) && ok) {
        spent = pr3_bench_time_calls(b, fn, ctx, calls, &ok);
        if (spent < UINT64_C(50000000)) calls *= 2;
    }
    if (!ok) {
        (void)printf("%-28s FAILED: the workload returned an error\n", workload);
        b->failures++;
        return;
    }
    double per_call = (double)spent / (double)calls;
    double target = b->min_ms * 1e6;
    calls = per_call > 0 ? (uint64_t)(target / per_call) + 1 : 1;

    static double ns[PR3_BENCH_MAX_RUNS];
    unsigned runs = b->runs == 0 || b->runs > PR3_BENCH_MAX_RUNS ? 30 : b->runs;
    for (unsigned r = 0; r < runs && ok; r++) {
        uint64_t elapsed = pr3_bench_time_calls(b, fn, ctx, calls, &ok);
        ns[r] = (double)elapsed / (double)calls;
        if (b->out) {
            double mbps = bytes ? (double)bytes / ns[r] * 1e3 : 0.0; /* bytes per ns = GB/s */
            (void)fprintf(b->out, "{\"subject\":\"pr3\",\"module\":");
            pr3_bench_put_json(b->out, b->module);
            (void)fprintf(b->out, ",\"workload\":");
            pr3_bench_put_json(b->out, workload);
            (void)fprintf(b->out, ",\"repetition\":%u,\"seed\":0,\"device\":", r);
            pr3_bench_put_json(b->out, b->device);
            (void)fprintf(b->out, ",\"model\":null,\"configuration\":");
            pr3_bench_put_json(b->out, PR3_BENCH_CONFIG ", " PR3_BENCH_COMPILER);
            (void)fprintf(
                b->out,
                ",\"cache\":\"warm\",\"outcome\":\"%s\",\"elapsed_ns\":%llu,\"calls\":%llu,"
                "\"bytes_per_call\":%zu,\"ns_per_call\":%.3f,\"mb_per_s\":%.3f,\"collector\":\"bench/c\"}\n",
                ok ? "ok" : "error", (unsigned long long)elapsed, (unsigned long long)calls, bytes, ns[r],
                mbps);
        }
    }
    if (!ok) {
        (void)printf("%-28s FAILED during the timed runs\n", workload);
        b->failures++;
        return;
    }
    qsort(ns, runs, sizeof ns[0], pr3_bench_cmp);
    double med = pr3_bench_quantile(ns, runs, 0.5);
    double iqr = pr3_bench_quantile(ns, runs, 0.75) - pr3_bench_quantile(ns, runs, 0.25);
    double mbps = bytes ? (double)bytes / med * 1e3 : 0.0;
    (void)printf(
        "%-28s %12.1f ns/call %10.1f MB/s   min %.1f  max %.1f  IQR %.1f%%  (%u runs x %llu calls)\n",
        workload, med, mbps, ns[0], ns[runs - 1], med > 0 ? iqr / med * 100.0 : 0.0, runs,
        (unsigned long long)calls);
}

/* Reads workloads/<rel> whole; NULL (after a message) if it cannot. */
static inline char *pr3_bench_read(pr3_bench *b, const char *rel, size_t *len) {
    char path[1024];
    (void)snprintf(path, sizeof path, "%s/%s", b->workloads, rel);
    FILE *f = fopen(path, "rb");
    if (!f) {
        (void)fprintf(stderr, "bench: cannot open %s\n", path);
        b->failures++;
        return NULL;
    }
    long size = fseek(f, 0, SEEK_END) == 0 ? ftell(f) : -1;
    char *data = size >= 0 && fseek(f, 0, SEEK_SET) == 0 ? (char *)malloc((size_t)size + 1) : NULL;
    bool ok = data && fread(data, 1, (size_t)size, f) == (size_t)size;
    (void)fclose(f);
    if (!ok) {
        free(data);
        (void)fprintf(stderr, "bench: cannot read %s\n", path);
        b->failures++;
        return NULL;
    }
    data[size] = '\0'; // NOLINT(clang-analyzer-security.ArrayBound): data holds size + 1 bytes
    *len = (size_t)size;
    return data;
}

/* --runs N (default 30, at most 1000), --min-ms MS (default 100), --out FILE (append
 * samples), --device TEXT, --workloads DIR, and an optional workload filter. */
static inline bool pr3_bench_init(pr3_bench *b, const char *module, int argc, char **argv) {
    memset(b, 0, sizeof *b);
    b->module = module;
    b->workloads = PR3_BENCH_WORKLOADS;
    b->runs = 30;
    b->min_ms = 100.0;
    pr3_bench_describe_device(b->device, sizeof b->device);
    for (int i = 1; i < argc; i++) {
        if (argv[i] == NULL) continue;
        const char *next = i + 1 < argc ? argv[i + 1] : NULL;
        if (strcmp(argv[i], "--runs") == 0 && next) {
            b->runs = (unsigned)strtoul(next, NULL, 10);
            i++;
        } else if (strcmp(argv[i], "--min-ms") == 0 && next) {
            b->min_ms = strtod(next, NULL);
            i++;
        } else if (strcmp(argv[i], "--out") == 0 && next) {
            if (b->out) (void)fclose(b->out); /* the last --out wins */
            b->out = fopen(next, "ab");
            if (!b->out) {
                (void)fprintf(stderr, "bench: cannot write %s\n", next);
                return false;
            }
            i++;
        } else if (strcmp(argv[i], "--device") == 0 && next) {
            (void)snprintf(b->device, sizeof b->device, "%s", next);
            i++;
        } else if (strcmp(argv[i], "--workloads") == 0 && next) {
            b->workloads = next;
            i++;
        } else if (argv[i][0] != '-') {
            b->filter = argv[i];
        } else {
            (void)fprintf(stderr, "bench: unknown option %s\n", argv[i]);
            if (b->out) (void)fclose(b->out);
            b->out = NULL;
            return false;
        }
    }
    if (b->runs == 0 || b->runs > PR3_BENCH_MAX_RUNS) b->runs = 30;
    (void)printf("# %s on %s, %s, %s; %u runs of at least %.0f ms\n", module, b->device, PR3_BENCH_CONFIG,
                 PR3_BENCH_COMPILER, b->runs, b->min_ms);
    return true;
}

static inline int pr3_bench_finish(pr3_bench *b) {
    if (b->out) (void)fclose(b->out);
    return b->failures ? 1 : 0;
}

#endif /* PR3_BENCH_H */
