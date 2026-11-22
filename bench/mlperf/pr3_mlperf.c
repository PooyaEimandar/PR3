/* pr3_mlperf: runs one MLPerf Inference test through LoadGen against a PR3 system under
 * test, and writes LoadGen's logs and a manifest.json to --out (doc/15-benchmarks.md).
 *
 * Exit status: 0 the run finished and, in performance mode, LoadGen judged it VALID;
 * 1 the run finished but was INVALID; 2 bad usage; 3 the run could not be done. */
#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>

#include "pr3_loadgen.h"
#include "sut_loopback.h"

#ifndef PR3_VERSION_STRING
#define PR3_VERSION_STRING "0.0.0"
#endif

enum { EXIT_VALID = 0, EXIT_INVALID = 1, EXIT_USAGE = 2, EXIT_FAILED = 3 };
enum { PATH_CAP = 4096 };

typedef struct options {
    const char *sut;
    const char *model;
    pr3_lg_scenario scenario;
    pr3_lg_mode mode;
    const char *user_conf;
    const char *out_dir;
    size_t total_count;
    size_t performance_count;
    pr3_loopback_options loopback;
    bool quiet;
} options;

static const char *const usage_text =
    "usage: pr3_mlperf --sut loopback --out DIR [options]\n"
    "  --sut NAME            loopback (llamacpp, pr3-llamacpp and pr3-engine join with their issues)\n"
    "  --scenario NAME       SingleStream (default) or Offline\n"
    "  --mode NAME           performance (default), accuracy or submission\n"
    "  --model KEY           benchmark key in mlperf.conf (default llama3_1-8b-edge)\n"
    "  --user-conf FILE      user.conf with the per-system settings the rules allow\n"
    "  --out DIR             run directory, created if missing\n"
    "  --total N             samples in the dataset (default 5000)\n"
    "  --performance N       samples held in memory at once (default 5000)\n"
    "  --tokens N            loopback: output tokens per sample (default 16)\n"
    "  --ttft-us N           loopback: delay before the first token (default 1000)\n"
    "  --tpot-us N           loopback: delay per further token (default 100)\n"
    "  --quiet               do not copy LoadGen's summary to stdout\n";

static bool parse_u64(const char *text, uint64_t max, uint64_t *out) {
    if (text == NULL || *text == '\0' || *text == '-' || *text == '+') {
        return false;
    }
    char *end = NULL;
    errno = 0;
    const unsigned long long v = strtoull(text, &end, 10);
    if (errno != 0 || end == NULL || *end != '\0' || v > max) {
        return false;
    }
    *out = (uint64_t)v;
    return true;
}

static bool parse_size(const char *text, size_t max, size_t *out) {
    uint64_t v = 0;
    if (!parse_u64(text, (uint64_t)max, &v)) {
        return false;
    }
    *out = (size_t)v;
    return true;
}

static bool parse_u32(const char *text, uint32_t max, uint32_t *out) {
    uint64_t v = 0;
    if (!parse_u64(text, (uint64_t)max, &v)) {
        return false;
    }
    *out = (uint32_t)v;
    return true;
}

static bool parse_scenario(const char *text, pr3_lg_scenario *out) {
    if (text != NULL && strcmp(text, "SingleStream") == 0) {
        *out = PR3_LG_SINGLE_STREAM;
        return true;
    }
    if (text != NULL && strcmp(text, "Offline") == 0) {
        *out = PR3_LG_OFFLINE;
        return true;
    }
    return false;
}

static bool parse_mode(const char *text, pr3_lg_mode *out) {
    if (text == NULL) {
        return false;
    }
    if (strcmp(text, "performance") == 0) {
        *out = PR3_LG_PERFORMANCE;
    } else if (strcmp(text, "accuracy") == 0) {
        *out = PR3_LG_ACCURACY;
    } else if (strcmp(text, "submission") == 0) {
        *out = PR3_LG_SUBMISSION;
    } else {
        return false;
    }
    return true;
}

static const char *mode_name(pr3_lg_mode mode) {
    switch (mode) {
    case PR3_LG_PERFORMANCE: return "performance";
    case PR3_LG_ACCURACY: return "accuracy";
    case PR3_LG_SUBMISSION: return "submission";
    }
    return "performance";
}

static int parse_args(int argc, char **argv, options *o) {
    *o = (options){
        .sut = NULL,
        .model = "llama3_1-8b-edge",
        .scenario = PR3_LG_SINGLE_STREAM,
        .mode = PR3_LG_PERFORMANCE,
        .total_count = 5000,
        .performance_count = 5000,
        .loopback = {.tokens = 16, .ttft_us = 1000, .tpot_us = 100, .queue_capacity = 65536},
    };
    for (int i = 1; i < argc; ++i) {
        const char *arg = argv[i];
        const char *val = (i + 1 < argc) ? argv[i + 1] : NULL;
        bool ok = true;
        bool takes_value = true;
        if (strcmp(arg, "--help") == 0 || strcmp(arg, "-h") == 0) {
            fputs(usage_text, stdout);
            return -1;
        } else if (strcmp(arg, "--quiet") == 0) {
            o->quiet = true;
            takes_value = false;
        } else if (strcmp(arg, "--sut") == 0) {
            o->sut = val;
            ok = val != NULL;
        } else if (strcmp(arg, "--model") == 0) {
            o->model = val;
            ok = val != NULL && *val != '\0';
        } else if (strcmp(arg, "--scenario") == 0) {
            ok = parse_scenario(val, &o->scenario);
        } else if (strcmp(arg, "--mode") == 0) {
            ok = parse_mode(val, &o->mode);
        } else if (strcmp(arg, "--user-conf") == 0) {
            o->user_conf = val;
            ok = val != NULL && *val != '\0';
        } else if (strcmp(arg, "--out") == 0) {
            o->out_dir = val;
            ok = val != NULL && *val != '\0';
        } else if (strcmp(arg, "--total") == 0) {
            ok = parse_size(val, (size_t)1 << 32, &o->total_count);
        } else if (strcmp(arg, "--performance") == 0) {
            ok = parse_size(val, (size_t)1 << 32, &o->performance_count);
        } else if (strcmp(arg, "--tokens") == 0) {
            ok = parse_u32(val, 4096, &o->loopback.tokens);
        } else if (strcmp(arg, "--ttft-us") == 0) {
            ok = parse_u32(val, 60u * 1000u * 1000u, &o->loopback.ttft_us);
        } else if (strcmp(arg, "--tpot-us") == 0) {
            ok = parse_u32(val, 10u * 1000u * 1000u, &o->loopback.tpot_us);
        } else {
            fprintf(stderr, "pr3_mlperf: unknown option %s\n", arg);
            return EXIT_USAGE;
        }
        if (!ok) {
            fprintf(stderr, "pr3_mlperf: bad or missing value for %s\n", arg);
            return EXIT_USAGE;
        }
        if (takes_value) {
            i += 1;
        }
    }
    if (o->sut == NULL || o->out_dir == NULL) {
        fputs(usage_text, stderr);
        return EXIT_USAGE;
    }
    if (o->total_count == 0 || o->performance_count == 0 || o->performance_count > o->total_count ||
        o->loopback.tokens == 0) {
        fprintf(stderr, "pr3_mlperf: counts must be positive and --performance at most --total\n");
        return EXIT_USAGE;
    }
    return 0;
}

/* mkdir -p for POSIX paths. */
static bool make_dirs(const char *path) {
    char buf[PATH_CAP];
    const size_t len = strlen(path);
    if (len == 0 || len >= sizeof buf) {
        return false;
    }
    memcpy(buf, path, len + 1);
    for (size_t i = 1; i <= len; ++i) {
        if (buf[i] == '/' || buf[i] == '\0') {
            const char saved = buf[i];
            buf[i] = '\0';
            if (mkdir(buf, 0755) != 0 && errno != EEXIST) {
                return false;
            }
            buf[i] = saved;
        }
    }
    struct stat st;
    return stat(path, &st) == 0 && S_ISDIR(st.st_mode);
}

static bool join_path(char *out, size_t cap, const char *dir, const char *name) {
    const int n = snprintf(out, cap, "%s/%s", dir, name);
    return n > 0 && (size_t)n < cap;
}

/* "VALID", "INVALID", or "UNKNOWN" from LoadGen's summary. */
static const char *read_result(const char *out_dir) {
    char path[PATH_CAP];
    if (!join_path(path, sizeof path, out_dir, "mlperf_log_summary.txt")) {
        return "UNKNOWN";
    }
    FILE *f = fopen(path, "r");
    if (f == NULL) {
        return "UNKNOWN";
    }
    const char *result = "UNKNOWN";
    char line[512];
    while (fgets(line, sizeof line, f) != NULL) {
        if (strncmp(line, "Result is : ", 12) == 0) {
            result = (strncmp(line + 12, "VALID", 5) == 0) ? "VALID" : "INVALID";
            break;
        }
    }
    fclose(f);
    return result;
}

static bool file_exists(const char *dir, const char *name) {
    char path[PATH_CAP];
    struct stat st;
    return join_path(path, sizeof path, dir, name) && stat(path, &st) == 0 && S_ISREG(st.st_mode);
}

/* A JSON string with the escapes of PR3's canonical JSON (doc/04-core-runtime.md). */
static void write_json_string(FILE *f, const char *s) {
    fputc('"', f);
    for (const unsigned char *p = (const unsigned char *)s; *p != '\0'; ++p) {
        switch (*p) {
        case '"': fputs("\\\"", f); break;
        case '\\': fputs("\\\\", f); break;
        case '\b': fputs("\\b", f); break;
        case '\f': fputs("\\f", f); break;
        case '\n': fputs("\\n", f); break;
        case '\r': fputs("\\r", f); break;
        case '\t': fputs("\\t", f); break;
        default:
            if (*p < 0x20) {
                fprintf(f, "\\u%04x", (unsigned)*p);
            } else {
                fputc(*p, f);
            }
        }
    }
    fputc('"', f);
}

/* manifest.json, keys in byte order, beside LoadGen's own logs. */
static bool write_manifest(const options *o, const char *result, uint64_t wall_ms, uint64_t completed) {
    char path[PATH_CAP];
    if (!join_path(path, sizeof path, o->out_dir, "manifest.json")) {
        return false;
    }
    FILE *f = fopen(path, "w");
    if (f == NULL) {
        return false;
    }
    fputs("{\"completed_samples\":", f);
    fprintf(f, "%llu", (unsigned long long)completed);
    fputs(",\"harness\":\"pr3_mlperf\",\"loadgen_git_revision\":", f);
    write_json_string(f, pr3_lg_git_revision());
    fputs(",\"loadgen_version\":", f);
    write_json_string(f, pr3_lg_version());
    if (strcmp(o->sut, "loopback") == 0) {
        fprintf(f, ",\"loopback\":{\"tokens\":%u,\"tpot_us\":%u,\"ttft_us\":%u}",
                (unsigned)o->loopback.tokens, (unsigned)o->loopback.tpot_us, (unsigned)o->loopback.ttft_us);
    }
    fputs(",\"mode\":", f);
    write_json_string(f, mode_name(o->mode));
    fputs(",\"model\":", f);
    write_json_string(f, o->model);
    fputs(",\"pr3_version\":", f);
    write_json_string(f, PR3_VERSION_STRING);
    fprintf(f, ",\"qsl_performance_count\":%llu,\"qsl_total_count\":%llu",
            (unsigned long long)o->performance_count, (unsigned long long)o->total_count);
    fputs(",\"result\":", f);
    write_json_string(f, result);
    fputs(",\"scenario\":", f);
    write_json_string(f, pr3_lg_scenario_name(o->scenario));
    fputs(",\"sut\":", f);
    write_json_string(f, o->sut);
    fputs(",\"user_conf\":", f);
    if (o->user_conf != NULL) {
        write_json_string(f, o->user_conf);
    } else {
        fputs("null", f);
    }
    fprintf(f, ",\"wall_ms\":%llu}\n", (unsigned long long)wall_ms);
    return fclose(f) == 0;
}

static uint64_t now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000u + (uint64_t)ts.tv_nsec / 1000000u;
}

/* The QSL for the loopback SUT has nothing to load. */
static void qsl_noop(void *ctx, const size_t *indices, size_t count) {
    (void)ctx;
    (void)indices;
    (void)count;
}

int main(int argc, char **argv) {
    options o;
    const int parsed = parse_args(argc, argv, &o);
    if (parsed < 0) {
        return EXIT_VALID; /* --help */
    }
    if (parsed != 0) {
        return parsed;
    }
    if (strcmp(o.sut, "loopback") != 0) {
        fprintf(stderr, "pr3_mlperf: the %s system under test is not built yet (doc/17-delivery-plan.md)\n",
                o.sut);
        return EXIT_USAGE;
    }
    if (!make_dirs(o.out_dir)) {
        fprintf(stderr, "pr3_mlperf: cannot create %s\n", o.out_dir);
        return EXIT_FAILED;
    }

    pr3_loopback *lb = pr3_loopback_create(&o.loopback);
    if (lb == NULL) {
        fprintf(stderr, "pr3_mlperf: cannot start the loopback system under test\n");
        return EXIT_FAILED;
    }
    const pr3_lg_sut sut = pr3_loopback_sut(lb);
    const pr3_lg_qsl qsl = {
        .name = "pr3-loopback-qsl",
        .ctx = NULL,
        .total_count = o.total_count,
        .performance_count = o.performance_count,
        .load = qsl_noop,
        .unload = qsl_noop,
    };
    const pr3_lg_config cfg = {
        .model = o.model,
        .scenario = o.scenario,
        .mode = o.mode,
        .user_conf = o.user_conf,
        .audit_conf = NULL,
        .out_dir = o.out_dir,
        .summary_to_stdout = !o.quiet,
    };

    const uint64_t start = now_ms();
    const pr3_lg_status status = pr3_lg_run(&sut, &qsl, &cfg);
    const uint64_t wall_ms = now_ms() - start;
    const uint64_t completed = pr3_loopback_completed(lb);
    pr3_loopback_destroy(lb);

    if (status != PR3_LG_OK) {
        fprintf(stderr, "pr3_mlperf: LoadGen run failed (status %d)\n", (int)status);
        return status == PR3_LG_BAD_CONFIG ? EXIT_USAGE : EXIT_FAILED;
    }

    const char *result = "UNKNOWN";
    if (o.mode == PR3_LG_ACCURACY) {
        result = file_exists(o.out_dir, "mlperf_log_accuracy.json") ? "COMPLETED" : "UNKNOWN";
    } else {
        result = read_result(o.out_dir);
    }
    if (!write_manifest(&o, result, wall_ms, completed)) {
        fprintf(stderr, "pr3_mlperf: cannot write manifest.json in %s\n", o.out_dir);
        return EXIT_FAILED;
    }
    fprintf(stderr, "pr3_mlperf: %s %s %s: %s, %llu samples in %llu ms (LoadGen %s)\n", o.sut,
            pr3_lg_scenario_name(o.scenario), mode_name(o.mode), result, (unsigned long long)completed,
            (unsigned long long)wall_ms, pr3_lg_version());

    if (strcmp(result, "VALID") == 0 || strcmp(result, "COMPLETED") == 0) {
        return EXIT_VALID;
    }
    return strcmp(result, "INVALID") == 0 ? EXIT_INVALID : EXIT_FAILED;
}
