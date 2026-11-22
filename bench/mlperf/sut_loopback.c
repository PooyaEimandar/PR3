/* The loopback system under test (sut_loopback.h). One worker thread answers samples in
 * the order LoadGen issued them; LoadGen's issue thread only enqueues. */
#define _POSIX_C_SOURCE 200809L

#include "sut_loopback.h"

#include <errno.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdlib.h>
#include <time.h>

enum { LOOPBACK_MAX_TOKENS = 4096 };

struct pr3_loopback {
    pr3_loopback_options options;
    pthread_t worker;
    pthread_mutex_t lock;
    pthread_cond_t not_empty;
    pthread_cond_t not_full;
    pr3_lg_sample *queue; /* ring of options.queue_capacity samples */
    size_t head;          /* next sample to answer */
    size_t count;         /* samples waiting */
    bool stopping;
    uint64_t completed;
    int32_t tokens[LOOPBACK_MAX_TOKENS];
};

static void sleep_us(uint32_t us) {
    if (us == 0) {
        return;
    }
    struct timespec left = {.tv_sec = (time_t)(us / 1000000u), .tv_nsec = (long)(us % 1000000u) * 1000L};
    while (nanosleep(&left, &left) != 0 && errno == EINTR) {}
}

/* Deterministic token ids for a sample, so accuracy logs are reproducible. */
static void fill_tokens(int32_t *tokens, uint32_t n, size_t index) {
    for (uint32_t i = 0; i < n; ++i) {
        tokens[i] = (int32_t)(((uint64_t)index * 31u + i) % 128000u);
    }
}

static void answer(pr3_loopback *lb, pr3_lg_sample sample) {
    const uint32_t n = lb->options.tokens;
    fill_tokens(lb->tokens, n, sample.index);

    sleep_us(lb->options.ttft_us);
    const pr3_lg_response first = {
        .id = sample.id, .data = lb->tokens, .size = sizeof lb->tokens[0], .n_tokens = 1};
    pr3_lg_first_token(&first, 1);

    for (uint32_t i = 1; i < n; ++i) {
        sleep_us(lb->options.tpot_us);
    }
    const pr3_lg_response done = {.id = sample.id,
                                  .data = lb->tokens,
                                  .size = (size_t)n * sizeof lb->tokens[0],
                                  .n_tokens = (int64_t)n};
    pr3_lg_complete(&done, 1);
}

static void *worker_main(void *arg) {
    pr3_loopback *lb = arg;
    for (;;) {
        pthread_mutex_lock(&lb->lock);
        while (lb->count == 0 && !lb->stopping) {
            pthread_cond_wait(&lb->not_empty, &lb->lock);
        }
        if (lb->count == 0) { /* stopping and drained */
            pthread_mutex_unlock(&lb->lock);
            return NULL;
        }
        const pr3_lg_sample sample = lb->queue[lb->head];
        lb->head = (lb->head + 1) % lb->options.queue_capacity;
        lb->count -= 1;
        pthread_cond_signal(&lb->not_full);
        pthread_mutex_unlock(&lb->lock);

        answer(lb, sample);

        pthread_mutex_lock(&lb->lock);
        lb->completed += 1;
        pthread_mutex_unlock(&lb->lock);
    }
}

static void loopback_issue(void *ctx, const pr3_lg_sample *samples, size_t count) {
    pr3_loopback *lb = ctx;
    pthread_mutex_lock(&lb->lock);
    for (size_t i = 0; i < count; ++i) {
        while (lb->count == lb->options.queue_capacity) {
            pthread_cond_wait(&lb->not_full, &lb->lock);
        }
        const size_t tail = (lb->head + lb->count) % lb->options.queue_capacity;
        lb->queue[tail] = samples[i];
        lb->count += 1;
        pthread_cond_signal(&lb->not_empty);
    }
    pthread_mutex_unlock(&lb->lock);
}

static void loopback_flush(void *ctx) {
    (void)ctx; /* every sample is answered as soon as the worker reaches it */
}

pr3_loopback *pr3_loopback_create(const pr3_loopback_options *options) {
    if (options == NULL || options->tokens == 0 || options->tokens > LOOPBACK_MAX_TOKENS ||
        options->queue_capacity == 0 || options->queue_capacity > ((size_t)1 << 24)) {
        return NULL;
    }
    pr3_loopback *lb = calloc(1, sizeof *lb);
    if (lb == NULL) {
        return NULL;
    }
    lb->options = *options;
    lb->queue = calloc(options->queue_capacity, sizeof *lb->queue);
    if (lb->queue == NULL) {
        free(lb);
        return NULL;
    }
    if (pthread_mutex_init(&lb->lock, NULL) != 0) {
        goto fail_queue;
    }
    if (pthread_cond_init(&lb->not_empty, NULL) != 0) {
        goto fail_lock;
    }
    if (pthread_cond_init(&lb->not_full, NULL) != 0) {
        goto fail_not_empty;
    }
    if (pthread_create(&lb->worker, NULL, worker_main, lb) != 0) {
        goto fail_not_full;
    }
    return lb;

fail_not_full:
    pthread_cond_destroy(&lb->not_full);
fail_not_empty:
    pthread_cond_destroy(&lb->not_empty);
fail_lock:
    pthread_mutex_destroy(&lb->lock);
fail_queue:
    free(lb->queue);
    free(lb);
    return NULL;
}

void pr3_loopback_destroy(pr3_loopback *lb) {
    if (lb == NULL) {
        return;
    }
    pthread_mutex_lock(&lb->lock);
    lb->stopping = true;
    pthread_cond_broadcast(&lb->not_empty);
    pthread_mutex_unlock(&lb->lock);
    pthread_join(lb->worker, NULL);
    pthread_cond_destroy(&lb->not_full);
    pthread_cond_destroy(&lb->not_empty);
    pthread_mutex_destroy(&lb->lock);
    free(lb->queue);
    free(lb);
}

pr3_lg_sut pr3_loopback_sut(pr3_loopback *lb) {
    return (pr3_lg_sut){.name = "pr3-loopback", .ctx = lb, .issue = loopback_issue, .flush = loopback_flush};
}

uint64_t pr3_loopback_completed(pr3_loopback *lb) {
    pthread_mutex_lock(&lb->lock);
    const uint64_t n = lb->completed;
    pthread_mutex_unlock(&lb->lock);
    return n;
}
