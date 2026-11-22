/* The base library's behaviour (doc/03; ledger: doc/20, "Base library"). */
#include <pr3/base.h>

#include "pr3_test.h"

#if defined(__unix__) || defined(__APPLE__)
#include <pthread.h>
#define PR3_TEST_THREADS 1
#endif

/* A deterministic generator for randomized tests: SplitMix64. */
PR3_WRAPPING static uint64_t rng_next(uint64_t *s) {
    uint64_t z = (*s += UINT64_C(0x9E3779B97F4A7C15));
    z = (z ^ (z >> 30)) * UINT64_C(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)) * UINT64_C(0x94D049BB133111EB);
    return z ^ (z >> 31);
}

/* ---- status */

static void test_every_status_has_text(pr3_t *t) {
    for (int s = PR3_OK; s <= PR3_E_CANCELLED; s++) {
        pr3_str text = pr3_status_text((pr3_status)s);
        PR3_EXPECT(t, text.len > 0 && !pr3_str_eq(text, PR3_S("unknown status")));
    }
    PR3_EXPECT_STR(t, pr3_status_text(PR3_E_NOMEM), "out of memory");
}

/* ---- arenas */

static void test_arena_allocations_are_aligned_and_disjoint(pr3_t *t) {
    static uint8_t mem[4096];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    uint8_t *prev_end = mem;
    for (size_t i = 0; i < 40; i++) {
        size_t align = (size_t)1 << (i % 7), size = 1 + i * 3;
        uint8_t *p = pr3_arena_alloc(&a, size, align);
        PR3_REQUIRE(t, p != NULL);
        PR3_EXPECT_U64(t, (uintptr_t)p % align, 0);
        PR3_EXPECT(t, p >= prev_end && p + size <= mem + sizeof mem);
        memset(p, (int)i, size);
        prev_end = p + size;
    }
    PR3_EXPECT(t, a.used <= a.cap && a.high_water == a.used);
    pr3_arena_fini(&a);
}

static void test_a_full_arena_returns_null_and_stays_usable(pr3_t *t) {
    static uint8_t mem[256];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    PR3_EXPECT(t, pr3_arena_alloc(&a, 512, 1) == NULL);
    PR3_EXPECT_U64(t, a.used, 0);
    PR3_EXPECT(t, pr3_arena_alloc(&a, 64, 8) != NULL);
    size_t used = a.used;
    PR3_EXPECT(t, pr3_arena_alloc(&a, sizeof mem, 1) == NULL);
    PR3_EXPECT_U64(t, a.used, used);
    PR3_EXPECT(t, pr3_arena_alloc(&a, 16, 8) != NULL);
    pr3_arena_fini(&a);
}

static void test_arena_size_overflow_is_refused(pr3_t *t) {
    static uint8_t mem[64];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    PR3_EXPECT(t, pr3_arena_alloc_n(&a, SIZE_MAX / 2, 3, 1) == NULL);
    PR3_EXPECT(t, pr3_arena_alloc(&a, SIZE_MAX, 1) == NULL);
    PR3_EXPECT(t, pr3_arena_alloc(&a, SIZE_MAX - 4, 64) == NULL);
    PR3_EXPECT_U64(t, a.used, 0);
    pr3_arena_fini(&a);
}

static void test_arena_rewind_releases_only_what_came_after_the_mark(pr3_t *t) {
    static uint8_t mem[1024];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    char *keep = pr3_arena_alloc(&a, 10, 1);
    PR3_REQUIRE(t, keep != NULL);
    memcpy(keep, "keep this", 10);
    pr3_arena_mark m = pr3_arena_mark_get(&a);
    PR3_EXPECT(t, pr3_arena_alloc(&a, 500, 16) != NULL);
    size_t high = a.used;
    pr3_arena_rewind(&a, m);
    PR3_EXPECT_U64(t, a.used, m.used);
    PR3_EXPECT_U64(t, a.high_water, high);
    PR3_EXPECT(t, memcmp(keep, "keep this", 10) == 0);
    pr3_arena_reset(&a);
    PR3_EXPECT_U64(t, a.used, 0);
    PR3_EXPECT_U64(t, a.high_water, high);
    pr3_arena_fini(&a);
}

static void test_an_empty_arena_hands_out_only_zero_bytes(pr3_t *t) {
    pr3_arena a;
    pr3_arena_init(&a, NULL, 0, "empty");
    PR3_EXPECT(t, pr3_arena_alloc(&a, 1, 1) == NULL);
    PR3_EXPECT(t, pr3_arena_alloc(&a, 0, 1) != NULL);
    PR3_EXPECT_U64(t, pr3_arena_remaining(&a), 0);
    pr3_arena_fini(&a);
}

static void test_released_arena_memory_is_poisoned(pr3_t *t) {
#if PR3_ASAN_POISON
    static uint8_t mem[256];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    uint8_t *p = pr3_arena_alloc(&a, 32, 8);
    PR3_REQUIRE(t, p != NULL);
    PR3_EXPECT(t, !__asan_address_is_poisoned(p) && !__asan_address_is_poisoned(p + 31));
    PR3_EXPECT(t, __asan_address_is_poisoned(p + 32)); /* the red zone */
    pr3_arena_reset(&a);
    PR3_EXPECT(t, __asan_address_is_poisoned(p));
    pr3_arena_fini(&a);
    PR3_EXPECT(t, !__asan_address_is_poisoned(p));
#elif !defined(NDEBUG)
    static uint8_t mem[64];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    uint8_t *p = pr3_arena_alloc(&a, 16, 1);
    PR3_REQUIRE(t, p != NULL);
    memset(p, 0x11, 16);
    pr3_arena_reset(&a);
    for (size_t i = 0; i < 16; i++) PR3_EXPECT_U64(t, mem[i], 0xDD);
    pr3_arena_fini(&a);
#else
    PR3_SKIP(t, "release builds neither poison nor fill");
#endif
}

/* ---- pools */

typedef struct item {
    uint64_t a, b;
} item;

static void test_pool_handles_resolve_until_released(pr3_t *t) {
    static uint8_t mem[4096];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_pool p;
    PR3_REQUIRE(t, pr3_pool_init(&p, &a, sizeof(item), _Alignof(item), 8) == PR3_OK);
    pr3_handle h[8];
    for (uint32_t i = 0; i < 8; i++) {
        item *it = pr3_pool_acquire(&p, &h[i]);
        PR3_REQUIRE(t, it != NULL);
        PR3_EXPECT(t, !pr3_handle_is_null(h[i]));
        it->a = i;
    }
    PR3_EXPECT_U64(t, h[0].index, 0);
    for (uint32_t i = 0; i < 8; i++) {
        item *it = pr3_pool_get(&p, h[i]);
        PR3_REQUIRE(t, it != NULL);
        PR3_EXPECT_U64(t, it->a, i);
    }
    PR3_EXPECT(t, pr3_pool_get(&p, PR3_HANDLE_NULL) == NULL);
    PR3_EXPECT(t, pr3_pool_get(&p, (pr3_handle){99, 1}) == NULL);
    PR3_EXPECT_U64(t, p.live, 8);
    PR3_EXPECT_U64(t, p.high_water, 8);
    pr3_arena_fini(&a);
}

static void test_a_stale_handle_resolves_to_null_and_cannot_release(pr3_t *t) {
    static uint8_t mem[1024];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_pool p;
    PR3_REQUIRE(t, pr3_pool_init(&p, &a, sizeof(item), _Alignof(item), 2) == PR3_OK);
    pr3_handle old, fresh;
    PR3_REQUIRE(t, pr3_pool_acquire(&p, &old) != NULL);
    PR3_EXPECT(t, pr3_pool_release(&p, old) == PR3_OK);
    PR3_EXPECT(t, pr3_pool_get(&p, old) == NULL);
    PR3_EXPECT(t, pr3_pool_release(&p, old) == PR3_E_STATE);
    PR3_REQUIRE(t, pr3_pool_acquire(&p, &fresh) != NULL);
    PR3_EXPECT_U64(t, fresh.index, old.index); /* the slot is reused... */
    PR3_EXPECT(t, fresh.gen != old.gen);       /* ...under a new generation */
    PR3_EXPECT(t, pr3_pool_get(&p, old) == NULL);
    PR3_EXPECT(t, pr3_pool_get(&p, fresh) != NULL);
    pr3_arena_fini(&a);
}

static void test_a_full_pool_refuses_and_recovers_after_a_release(pr3_t *t) {
    static uint8_t mem[1024];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_pool p;
    PR3_REQUIRE(t, pr3_pool_init(&p, &a, 24, 8, 3) == PR3_OK);
    pr3_handle h[3], extra = {7, 7};
    for (int i = 0; i < 3; i++) PR3_REQUIRE(t, pr3_pool_acquire(&p, &h[i]) != NULL);
    PR3_EXPECT(t, pr3_pool_acquire(&p, &extra) == NULL);
    PR3_EXPECT(t, extra.index == 7 && extra.gen == 7); /* untouched on failure */
    PR3_EXPECT(t, pr3_pool_release(&p, h[1]) == PR3_OK);
    PR3_EXPECT(t, pr3_pool_acquire(&p, &extra) != NULL);
    PR3_EXPECT_U64(t, extra.index, h[1].index);
    pr3_arena_fini(&a);
}

static void test_acquired_slots_are_zeroed(pr3_t *t) {
    static uint8_t mem[1024];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_pool p;
    PR3_REQUIRE(t, pr3_pool_init(&p, &a, sizeof(item), _Alignof(item), 1) == PR3_OK);
    pr3_handle h;
    item *it = pr3_pool_acquire(&p, &h);
    PR3_REQUIRE(t, it != NULL);
    it->a = it->b = UINT64_MAX;
    PR3_EXPECT(t, pr3_pool_release(&p, h) == PR3_OK);
    it = pr3_pool_acquire(&p, &h);
    PR3_REQUIRE(t, it != NULL);
    PR3_EXPECT(t, it->a == 0 && it->b == 0);
    pr3_arena_fini(&a);
}

static void test_a_slot_whose_generation_would_wrap_is_retired(pr3_t *t) {
    static uint8_t mem[1024];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_pool p;
    PR3_REQUIRE(t, pr3_pool_init(&p, &a, 8, 8, 2) == PR3_OK);
    pr3_handle h;
    PR3_REQUIRE(t, pr3_pool_acquire(&p, &h) != NULL);
    p.gens[h.index] = UINT32_MAX; /* as if 2^31 acquire/release cycles had passed */
    h.gen = UINT32_MAX;
    PR3_EXPECT(t, pr3_pool_release(&p, h) == PR3_OK);
    PR3_EXPECT(t, pr3_pool_get(&p, h) == NULL);
    pr3_handle next;
    PR3_REQUIRE(t, pr3_pool_acquire(&p, &next) != NULL);
    PR3_EXPECT(t, next.index != h.index);
    pr3_handle none;
    PR3_EXPECT(t, pr3_pool_acquire(&p, &none) == NULL); /* the retired slot never returns */
    pr3_arena_fini(&a);
}

static void test_pool_init_leaves_a_small_arena_unchanged(pr3_t *t) {
    static uint8_t mem[4096];
    for (size_t cap = 0; cap < 600; cap += 7) {
        pr3_arena a;
        pr3_arena_init(&a, mem, cap, "test");
        pr3_pool p;
        pr3_status s = pr3_pool_init(&p, &a, 24, 8, 16);
        PR3_EXPECT(t, s == PR3_OK || (s == PR3_E_NOMEM && a.used == 0));
        pr3_arena_fini(&a);
    }
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_pool p;
    PR3_EXPECT(t, pr3_pool_init(&p, &a, 0, 8, 4) == PR3_E_INVALID);
    PR3_EXPECT(t, pr3_pool_init(&p, &a, 8, 3, 4) == PR3_E_INVALID);
    pr3_arena_fini(&a);
}

static void test_pool_bytes_covers_what_init_takes(pr3_t *t) {
    static uint8_t mem[1 << 16];
    static const size_t aligns[] = {1, 8, 16, 64};
    for (size_t ai = 0; ai < PR3_ARRAY_LEN(aligns); ai++) {
        for (uint32_t cap = 0; cap < 40; cap += 3) {
            size_t need = pr3_pool_bytes(40, aligns[ai], cap);
            PR3_REQUIRE(t, need > 0 && need <= sizeof mem);
            for (size_t offset = 0; offset < 8; offset++) {
                pr3_arena a;
                pr3_arena_init(&a, mem + offset, need, "test");
                pr3_pool p;
                PR3_EXPECT(t, pr3_pool_init(&p, &a, 40, aligns[ai], cap) == PR3_OK);
                pr3_arena_fini(&a);
            }
        }
    }
    PR3_EXPECT_U64(t, pr3_pool_bytes(SIZE_MAX / 2, 8, 4), 0);
}

/* ---- the buddy heap */

static uint8_t buddy_mem[(1 << 16) + 4096];

static bool buddy_setup(pr3_t *t, pr3_arena *a, pr3_buddy *b, size_t heap) {
    pr3_arena_init(a, buddy_mem, sizeof buddy_mem, "test");
    return PR3_EXPECT(t, pr3_buddy_init(b, a, heap, "heap") == PR3_OK);
}

static void test_buddy_blocks_are_powers_of_two_and_aligned(pr3_t *t) {
    pr3_arena a;
    pr3_buddy b;
    if (!buddy_setup(t, &a, &b, 1 << 16)) return;
    PR3_EXPECT_U64(t, pr3_buddy_block_size(0), 64);
    PR3_EXPECT_U64(t, pr3_buddy_block_size(64), 64);
    PR3_EXPECT_U64(t, pr3_buddy_block_size(65), 128);
    PR3_EXPECT_U64(t, pr3_buddy_block_size(5000), 8192);
    uint8_t *p = pr3_buddy_alloc(&b, 100);
    PR3_REQUIRE(t, p != NULL);
    PR3_EXPECT_U64(t, (uintptr_t)p % 64, 0);
    PR3_EXPECT_U64(t, b.used, 128);
    memset(p, 0xAB, 100);
    pr3_buddy_free(&b, p);
    PR3_EXPECT_U64(t, b.used, 0);
    PR3_EXPECT_U64(t, b.high_water, 128);
    pr3_arena_fini(&a);
}

static void test_buddy_coalesces_back_to_one_block(pr3_t *t) {
    pr3_arena a;
    pr3_buddy b;
    if (!buddy_setup(t, &a, &b, 1 << 16)) return;
    void *p[1024];
    size_t n = 0;
    while (n < 1024 && (p[n] = pr3_buddy_alloc(&b, 64)) != NULL) n++;
    PR3_EXPECT_U64(t, n, 1024); /* 64 KiB in 64-byte blocks */
    PR3_EXPECT(t, pr3_buddy_alloc(&b, 1) == NULL);
    for (size_t i = 0; i < n; i += 2) pr3_buddy_free(&b, p[i]); /* every other block */
    PR3_EXPECT(t, pr3_buddy_alloc(&b, 128) == NULL);            /* nothing coalesced yet */
    for (size_t i = 1; i < n; i += 2) pr3_buddy_free(&b, p[i]);
    void *whole = pr3_buddy_alloc(&b, 1 << 16);
    PR3_EXPECT(t, whole != NULL);
    PR3_EXPECT_U64(t, b.used, 1 << 16);
    pr3_buddy_free(&b, whole);
    pr3_arena_fini(&a);
}

static void test_a_full_buddy_heap_returns_null(pr3_t *t) {
    pr3_arena a;
    pr3_buddy b;
    if (!buddy_setup(t, &a, &b, 4096)) return;
    PR3_EXPECT(t, pr3_buddy_alloc(&b, 4097) == NULL);
    void *x = pr3_buddy_alloc(&b, 2048), *y = pr3_buddy_alloc(&b, 1024);
    PR3_EXPECT(t, x != NULL && y != NULL);
    PR3_EXPECT(t, pr3_buddy_alloc(&b, 2048) == NULL);
    PR3_EXPECT(t, pr3_buddy_alloc(&b, 1024) != NULL);
    PR3_EXPECT(t, pr3_buddy_alloc(&b, 64) == NULL);
    pr3_arena_fini(&a);
}

static void test_buddy_random_traffic_never_overlaps_and_accounts_exactly(pr3_t *t) {
    pr3_arena a;
    pr3_buddy b;
    if (!buddy_setup(t, &a, &b, 1 << 16)) return;
    enum { SLOTS = 96 };
    uint8_t *ptr[SLOTS] = {0};
    size_t size[SLOTS] = {0};
    uint64_t seed = 0x9E3779B97F4A7C15u;
    for (int step = 0; step < 20000; step++) {
        size_t i = (size_t)(rng_next(&seed) % SLOTS);
        if (ptr[i]) {
            for (size_t k = 0; k < size[i]; k++) PR3_REQUIRE(t, ptr[i][k] == (uint8_t)i);
            pr3_buddy_free(&b, ptr[i]);
            ptr[i] = NULL;
        } else {
            uint64_t range = rng_next(&seed) % 4 == 0 ? 6000 : 300;
            size_t want = 1 + (size_t)(rng_next(&seed) % range);
            ptr[i] = pr3_buddy_alloc(&b, want);
            if (ptr[i]) {
                size[i] = want;
                memset(ptr[i], (int)i, want);
            }
        }
        size_t used = 0;
        for (size_t k = 0; k < SLOTS; k++)
            if (ptr[k]) used += pr3_buddy_block_size(size[k]);
        PR3_REQUIRE(t, used == b.used);
    }
    for (size_t i = 0; i < SLOTS; i++) pr3_buddy_free(&b, ptr[i]);
    PR3_EXPECT_U64(t, b.used, 0);
    PR3_EXPECT(t, pr3_buddy_alloc(&b, 1 << 16) != NULL);
    pr3_arena_fini(&a);
}

static void test_buddy_bytes_covers_what_init_takes(pr3_t *t) {
    for (size_t heap = 64; heap <= (1 << 16); heap <<= 1) {
        size_t need = pr3_buddy_bytes(heap);
        PR3_REQUIRE(t, need >= heap && need <= sizeof buddy_mem);
        for (size_t offset = 0; offset < 64; offset += 9) {
            pr3_arena a;
            pr3_arena_init(&a, buddy_mem + offset, need, "test");
            pr3_buddy b;
            PR3_EXPECT(t, pr3_buddy_init(&b, &a, heap, "heap") == PR3_OK);
            pr3_arena_fini(&a);
        }
    }
}

static void test_buddy_rejects_sizes_that_are_not_powers_of_two(pr3_t *t) {
    pr3_arena a;
    pr3_arena_init(&a, buddy_mem, sizeof buddy_mem, "test");
    pr3_buddy b;
    PR3_EXPECT(t, pr3_buddy_init(&b, &a, 3000, "heap") == PR3_E_INVALID);
    PR3_EXPECT(t, pr3_buddy_init(&b, &a, 32, "heap") == PR3_E_INVALID);
    PR3_EXPECT(t, pr3_buddy_init(&b, &a, 1 << 20, "heap") == PR3_E_NOMEM);
    PR3_EXPECT_U64(t, a.used, 0);
    PR3_EXPECT_U64(t, pr3_buddy_bytes(100), 0);
    pr3_arena_fini(&a);
}

/* ---- rings */

static void test_ring_is_fifo_and_reports_full_and_empty(pr3_t *t) {
    static uint8_t mem[1024];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_ring r;
    PR3_REQUIRE(t, pr3_ring_init(&r, &a, sizeof(uint32_t), _Alignof(uint32_t), 4) == PR3_OK);
    PR3_EXPECT(t, pr3_ring_peek(&r) == NULL);
    for (uint32_t i = 0; i < 4; i++) {
        uint32_t *slot = pr3_ring_reserve(&r);
        PR3_REQUIRE(t, slot != NULL);
        *slot = i;
        pr3_ring_publish(&r);
    }
    PR3_EXPECT(t, pr3_ring_reserve(&r) == NULL);
    for (uint32_t i = 0; i < 4; i++) {
        uint32_t *slot = pr3_ring_peek(&r);
        PR3_REQUIRE(t, slot != NULL);
        PR3_EXPECT_U64(t, *slot, i);
        pr3_ring_consume(&r);
    }
    PR3_EXPECT(t, pr3_ring_peek(&r) == NULL);
    pr3_arena_fini(&a);
}

static void test_ring_indices_wrap(pr3_t *t) {
    static uint8_t mem[1024];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_ring r;
    PR3_REQUIRE(t, pr3_ring_init(&r, &a, sizeof(uint32_t), _Alignof(uint32_t), 8) == PR3_OK);
    uint32_t start = UINT32_MAX - 5;
    r.head = start; /* single-threaded here: plain stores are fine */
    r.tail = start;
    r.head_cache = r.tail_cache = start;
    uint32_t next_in = 0, next_out = 0;
    for (int round = 0; round < 100; round++) {
        for (int k = 0; k < 5; k++) {
            uint32_t *slot = pr3_ring_reserve(&r);
            PR3_REQUIRE(t, slot != NULL);
            *slot = next_in++;
            pr3_ring_publish(&r);
        }
        for (int k = 0; k < 5; k++) {
            uint32_t *slot = pr3_ring_peek(&r);
            PR3_REQUIRE(t, slot != NULL);
            PR3_REQUIRE(t, *slot == next_out);
            next_out++;
            pr3_ring_consume(&r);
        }
    }
    PR3_EXPECT_U64(t, next_out, 500);
    pr3_arena_fini(&a);
}

static void test_ring_rejects_bad_capacity(pr3_t *t) {
    static uint8_t mem[256];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_ring r;
    PR3_EXPECT(t, pr3_ring_init(&r, &a, 8, 8, 3) == PR3_E_INVALID);
    PR3_EXPECT(t, pr3_ring_init(&r, &a, 8, 8, 1) == PR3_E_INVALID);
    PR3_EXPECT(t, pr3_ring_init(&r, &a, 0, 8, 4) == PR3_E_INVALID);
    PR3_EXPECT(t, pr3_ring_init(&r, &a, 64, 8, 8) == PR3_E_NOMEM);
    pr3_arena_fini(&a);
}

#if PR3_TEST_THREADS
enum { RING_ITEMS = 200000 };

static void *ring_producer(void *arg) {
    pr3_ring *r = arg;
    for (uint64_t i = 0; i < RING_ITEMS;) {
        uint64_t *slot = pr3_ring_reserve(r);
        if (!slot) continue;
        *slot = i++ * 3;
        pr3_ring_publish(r);
    }
    return NULL;
}
#endif

static void test_ring_transfers_across_threads(pr3_t *t) {
#if PR3_TEST_THREADS
    static uint8_t mem[4096];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    pr3_ring r;
    PR3_REQUIRE(t, pr3_ring_init(&r, &a, sizeof(uint64_t), _Alignof(uint64_t), 64) == PR3_OK);
    pthread_t producer;
    PR3_REQUIRE(t, pthread_create(&producer, NULL, ring_producer, &r) == 0);
    uint64_t expect = 0;
    bool in_order = true;
    while (expect < RING_ITEMS) {
        uint64_t *slot = pr3_ring_peek(&r);
        if (!slot) continue;
        in_order = in_order && *slot == expect * 3;
        expect++;
        pr3_ring_consume(&r);
    }
    pthread_join(producer, NULL);
    PR3_EXPECT(t, in_order);
    pr3_arena_fini(&a);
#else
    PR3_SKIP(t, "no threads on this target");
#endif
}

/* ---- builders and slices */

static void test_builder_overflow_is_sticky_and_reported_once(pr3_t *t) {
    char mem[8];
    pr3_buf b = pr3_buf_make(mem, sizeof mem);
    pr3_buf_put(&b, PR3_S("hello"));
    pr3_buf_put(&b, PR3_S(" world"));
    pr3_buf_put_char(&b, '!');
    PR3_EXPECT(t, b.overflow);
    PR3_EXPECT_U64(t, b.len, 5);
    pr3_str out = {0};
    PR3_EXPECT(t, pr3_buf_take(&b, &out) == PR3_E_LIMIT);
    PR3_EXPECT(t, out.ptr == NULL);
    pr3_buf exact = pr3_buf_make(mem, sizeof mem);
    pr3_buf_put(&exact, PR3_S("12345678"));
    PR3_EXPECT(t, pr3_buf_take(&exact, &out) == PR3_OK);
    PR3_EXPECT_STR(t, out, "12345678");
    pr3_buf none = pr3_buf_make(NULL, 0);
    pr3_buf_put(&none, PR3_S(""));
    PR3_EXPECT(t, pr3_buf_take(&none, &out) == PR3_OK && out.len == 0);
}

static void test_builder_writes_integers(pr3_t *t) {
    char mem[128];
    pr3_buf b = pr3_buf_make(mem, sizeof mem);
    pr3_buf_put_u64(&b, 0);
    pr3_buf_put_char(&b, ' ');
    pr3_buf_put_u64(&b, UINT64_MAX);
    pr3_buf_put_char(&b, ' ');
    pr3_buf_put_i64(&b, INT64_MIN);
    pr3_buf_put_char(&b, ' ');
    pr3_buf_put_i64(&b, -7);
    pr3_buf_put_char(&b, ' ');
    pr3_buf_put_i64(&b, INT64_MAX);
    pr3_str out;
    PR3_REQUIRE(t, pr3_buf_take(&b, &out) == PR3_OK);
    PR3_EXPECT_STR(t, out, "0 18446744073709551615 -9223372036854775808 -7 9223372036854775807");
}

static void test_trailing_zero_bits_are_counted(pr3_t *t) {
    for (unsigned i = 0; i < 64; i++) {
        uint64_t bit = UINT64_C(1) << i;
        PR3_EXPECT_U64(t, pr3_ctz64(bit), i);
        PR3_EXPECT_U64(t, pr3_ctz64(~(bit - 1)), i); /* every bit from i up */
        PR3_EXPECT_U64(t, pr3_ctz64(bit | UINT64_C(0x8000000000000000)), i);
    }
}

static void test_builder_writes_lowercase_hex(pr3_t *t) {
    char mem[16];
    pr3_buf b = pr3_buf_make(mem, sizeof mem);
    static const uint8_t bytes[] = {0x00, 0x7f, 0xab, 0xff};
    pr3_buf_put_hex(&b, (pr3_bytes){bytes, sizeof bytes});
    pr3_str out;
    PR3_REQUIRE(t, pr3_buf_take(&b, &out) == PR3_OK);
    PR3_EXPECT_STR(t, out, "007fabff");
}

static void test_clip_cuts_on_a_character_boundary_with_the_marker(pr3_t *t) {
    pr3_str s = PR3_S("ab\xc3\xa9\xe2\x82\xac"); /* "abé€" */
    PR3_EXPECT_U64(t, pr3_str_clip(s, 100).len, s.len);
    PR3_EXPECT_U64(t, pr3_str_clip(s, 3).len, 2);
    PR3_EXPECT_U64(t, pr3_str_clip(s, 4).len, 4);
    PR3_EXPECT_U64(t, pr3_str_clip(s, 6).len, 4);
    PR3_EXPECT_U64(t, pr3_str_clip(s, 0).len, 0);
    char mem[64];
    pr3_buf b = pr3_buf_make(mem, sizeof mem);
    pr3_buf_put_clip(&b, s, 5);
    pr3_str out;
    PR3_REQUIRE(t, pr3_buf_take(&b, &out) == PR3_OK);
    PR3_EXPECT_STR(t, out, "ab\xc3\xa9 \xe2\x80\xa6[truncated]");
    b = pr3_buf_make(mem, sizeof mem);
    pr3_buf_put_clip(&b, s, s.len);
    PR3_REQUIRE(t, pr3_buf_take(&b, &out) == PR3_OK);
    PR3_EXPECT_U64(t, out.len, s.len);
}

static void test_str_compare_is_byte_order(pr3_t *t) {
    PR3_EXPECT(t, pr3_str_cmp(PR3_S("a"), PR3_S("b")) < 0);
    PR3_EXPECT(t, pr3_str_cmp(PR3_S("ab"), PR3_S("a")) > 0);
    PR3_EXPECT(t, pr3_str_cmp(PR3_S(""), PR3_S("")) == 0);
    PR3_EXPECT(t, pr3_str_cmp(PR3_S("Z"), PR3_S("a")) < 0);
    PR3_EXPECT(t, pr3_str_cmp(PR3_S("\xc3\xa9"), PR3_S("z")) > 0);
    PR3_EXPECT(t, pr3_str_eq(PR3_S("x\0y"), PR3_S("x\0y")));
    PR3_EXPECT(t, !pr3_str_eq(PR3_S("x\0y"), PR3_S("x\0z")));
}

static void test_parse_u64_accepts_only_canonical_decimal(pr3_t *t) {
    uint64_t v = 42;
    PR3_EXPECT(t, pr3_parse_u64(PR3_S("0"), &v) == PR3_OK && v == 0);
    PR3_EXPECT(t, pr3_parse_u64(PR3_S("18446744073709551615"), &v) == PR3_OK && v == UINT64_MAX);
    v = 42;
    static const char *bad[] = {"", "01", "+1", "-1", " 1", "1 ", "1a", "0x1"};
    for (size_t i = 0; i < PR3_ARRAY_LEN(bad); i++) {
        pr3_str s = {bad[i], strlen(bad[i])};
        PR3_EXPECT(t, pr3_parse_u64(s, &v) == PR3_E_INVALID);
    }
    PR3_EXPECT_U64(t, v, 42);
}

static void test_parse_u64_refuses_overflow(pr3_t *t) {
    uint64_t v = 7;
    PR3_EXPECT(t, pr3_parse_u64(PR3_S("18446744073709551616"), &v) == PR3_E_LIMIT);
    PR3_EXPECT(t, pr3_parse_u64(PR3_S("99999999999999999999999"), &v) == PR3_E_LIMIT);
    PR3_EXPECT_U64(t, v, 7);
}

/* ---- UTF-8 */

static bool valid(const char *s, size_t n) { return pr3_utf8_validate((const uint8_t *)s, n) == PR3_OK; }
#define VALID(lit) valid(lit, sizeof(lit) - 1)

static void test_utf8_accepts_every_well_formed_sequence(pr3_t *t) {
    PR3_EXPECT(t, VALID(""));
    PR3_EXPECT(t, VALID("plain ASCII text, longer than eight bytes"));
    PR3_EXPECT(t, VALID("\x7f"));
    PR3_EXPECT(t, VALID("\xc2\x80"));
    PR3_EXPECT(t, VALID("\xdf\xbf"));
    PR3_EXPECT(t, VALID("\xe0\xa0\x80"));
    PR3_EXPECT(t, VALID("\xed\x9f\xbf"));     /* U+D7FF */
    PR3_EXPECT(t, VALID("\xee\x80\x80"));     /* U+E000 */
    PR3_EXPECT(t, VALID("\xef\xbf\xbf"));     /* U+FFFF */
    PR3_EXPECT(t, VALID("\xf0\x90\x80\x80")); /* U+10000 */
    PR3_EXPECT(t, VALID("\xf4\x8f\xbf\xbf")); /* U+10FFFF */
    PR3_EXPECT(t, VALID("\xd8\xb3\xd9\x84\xd8\xa7\xd9\x85 \xe2\x82\xac \xf0\x9f\x98\x80"));
}

static void test_utf8_rejects_overlongs_surrogates_and_out_of_range(pr3_t *t) {
    PR3_EXPECT(t, !VALID("\xc0\x80"));
    PR3_EXPECT(t, !VALID("\xc1\xbf"));
    PR3_EXPECT(t, !VALID("\xe0\x9f\xbf"));
    PR3_EXPECT(t, !VALID("\xed\xa0\x80")); /* U+D800 */
    PR3_EXPECT(t, !VALID("\xed\xbf\xbf")); /* U+DFFF */
    PR3_EXPECT(t, !VALID("\xf0\x8f\xbf\xbf"));
    PR3_EXPECT(t, !VALID("\xf4\x90\x80\x80")); /* U+110000 */
    PR3_EXPECT(t, !VALID("\xf5\x80\x80\x80"));
    PR3_EXPECT(t, !VALID("\xff"));
    PR3_EXPECT(t, !VALID("\x80"));
    PR3_EXPECT(t, !VALID("abcdefgh\x80"));
    PR3_EXPECT_U64(t, pr3_utf8_valid_prefix((const uint8_t *)"abc\xc3\x28", 5), 3);
}

static void test_utf8_rejects_truncated_sequences(pr3_t *t) {
    PR3_EXPECT(t, !VALID("\xc3"));
    PR3_EXPECT(t, !VALID("\xe2\x82"));
    PR3_EXPECT(t, !VALID("\xf0\x9f\x98"));
    PR3_EXPECT(t, !VALID("\xe2\x82x"));
    uint32_t cp = 99;
    PR3_EXPECT_U64(t, pr3_utf8_decode((const uint8_t *)"\xe2\x82\xac", 2, &cp), 0);
    PR3_EXPECT_U64(t, cp, 99);
}

static void test_utf8_encode_round_trips_every_scalar_value(pr3_t *t) {
    unsigned bad = 0;
    for (uint32_t cp = 0; cp <= 0x10FFFF; cp++) {
        uint8_t buf[4];
        size_t n = pr3_utf8_encode(cp, buf);
        if (cp >= 0xD800 && cp <= 0xDFFF) {
            bad += n != 0;
            continue;
        }
        uint32_t back = UINT32_MAX;
        bad +=
            n == 0 || pr3_utf8_decode(buf, n, &back) != n || back != cp || pr3_utf8_valid_prefix(buf, n) != n;
    }
    PR3_EXPECT_U64(t, bad, 0);
    uint8_t buf[4];
    PR3_EXPECT_U64(t, pr3_utf8_encode(0x110000, buf), 0);
}

static void test_utf8_validation_agrees_with_decoding_on_random_bytes(pr3_t *t) {
    uint64_t seed = 12345;
    uint8_t buf[64];
    unsigned disagree = 0;
    for (int round = 0; round < 20000; round++) {
        size_t n = (size_t)(rng_next(&seed) % sizeof buf);
        for (size_t i = 0; i < n; i++) {
            uint64_t r = rng_next(&seed);
            buf[i] = (uint8_t)(r % 4 == 0   ? 0x80 + (r >> 8) % 0x80
                               : r % 3 == 0 ? 0xC0 + (r >> 8) % 0x40
                                            : r >> 16);
        }
        size_t i = 0; /* the simple reference: decode one value at a time */
        while (i < n) {
            uint32_t cp;
            size_t len = pr3_utf8_decode(buf + i, n - i, &cp);
            if (len == 0) break;
            i += len;
        }
        disagree += pr3_utf8_valid_prefix(buf, n) != i;
    }
    PR3_EXPECT_U64(t, disagree, 0);
}

static void test_str_from_bytes_refuses_invalid_utf8(pr3_t *t) {
    pr3_str s = PR3_S("unchanged");
    static const uint8_t bad[] = {'o', 'k', 0xC3};
    PR3_EXPECT(t, pr3_str_from_bytes((pr3_bytes){bad, sizeof bad}, &s) == PR3_E_UTF8);
    PR3_EXPECT_STR(t, s, "unchanged");
    PR3_EXPECT(t, pr3_str_from_bytes((pr3_bytes){bad, 2}, &s) == PR3_OK);
    PR3_EXPECT_STR(t, s, "ok");
}

/* ---- SipHash */

static void test_siphash24_matches_the_papers_example(pr3_t *t) {
    uint8_t key[16], msg[15];
    for (int i = 0; i < 16; i++) key[i] = (uint8_t)i;
    for (int i = 0; i < 15; i++) msg[i] = (uint8_t)i;
    pr3_sipkey k = pr3_sipkey_from_bytes(key);
    PR3_EXPECT_U64(t, pr3_siphash24(k, msg, sizeof msg), UINT64_C(0xa129ca6149be45e5));
    PR3_EXPECT_U64(t, pr3_siphash24(k, msg, 0), UINT64_C(0x726fdb47dd0e0e31));
}

static void test_siphash_matches_the_golden_vectors(pr3_t *t) {
    pr3_test_lines l;
    if (!pr3_test_lines_open(t, &l, "base/siphash.jsonl")) return;
    const char *line;
    unsigned n = 0;
    while ((line = pr3_test_lines_next(&l)) != NULL) {
        int64_t c = 0, d = 0;
        size_t klen = 0, mlen = 0, olen = 0;
        unsigned char *key = pr3_test_field_hex(line, "key", &klen);
        unsigned char *msg = pr3_test_field_hex(line, "msg", &mlen);
        unsigned char *out = pr3_test_field_hex(line, "out", &olen);
        bool shape = key && msg && out && klen == 16 && olen == 8 && pr3_test_field_i64(line, "c", &c) &&
                     pr3_test_field_i64(line, "d", &d);
        if (PR3_EXPECT(t, shape)) {
            pr3_sipkey k = pr3_sipkey_from_bytes(key);
            pr3_siphash one;
            pr3_siphash_init(&one, k, (unsigned)c, (unsigned)d);
            pr3_siphash_update(&one, msg, mlen);
            uint64_t whole = pr3_siphash_final(&one);
            pr3_siphash split; /* the same bytes fed in two pieces */
            pr3_siphash_init(&split, k, (unsigned)c, (unsigned)d);
            pr3_siphash_update(&split, msg, mlen / 3);
            pr3_siphash_update(&split, msg + mlen / 3, mlen - mlen / 3);
            uint8_t got[8];
            pr3_store_le64(got, whole);
            PR3_EXPECT_MEM(t, got, 8, out, 8);
            PR3_EXPECT_U64(t, pr3_siphash_final(&split), whole);
            if (c == 1 && d == 3) PR3_EXPECT_U64(t, pr3_siphash13(k, msg, mlen), whole);
            if (c == 2 && d == 4) PR3_EXPECT_U64(t, pr3_siphash24(k, msg, mlen), whole);
        }
        free(key);
        free(msg);
        free(out);
        n++;
    }
    pr3_test_lines_close(&l);
    PR3_EXPECT_U64(t, n, 128);
}

/* ---- fixed-capacity lists */

static void test_vec_push_stops_at_capacity(pr3_t *t) {
    static uint8_t mem[256];
    pr3_arena a;
    pr3_arena_init(&a, mem, sizeof mem, "test");
    PR3_VEC(uint32_t) v;
    PR3_REQUIRE(t, PR3_VEC_INIT(&a, &v, uint32_t, 3) == PR3_OK);
    PR3_EXPECT(t, PR3_VEC_PUSH(&v, 1u) == PR3_OK);
    PR3_EXPECT(t, PR3_VEC_PUSH(&v, 2u) == PR3_OK);
    PR3_EXPECT(t, PR3_VEC_PUSH(&v, 3u) == PR3_OK);
    PR3_EXPECT(t, PR3_VEC_PUSH(&v, 4u) == PR3_E_LIMIT);
    PR3_EXPECT(t, v.len == 3 && v.items[2] == 3);
    PR3_EXPECT(t, PR3_VEC_INIT(&a, &v, uint32_t, 1000) == PR3_E_NOMEM);
    PR3_EXPECT_U64(t, v.cap, 0);
    pr3_arena_fini(&a);
}

PR3_TEST_MAIN(
    PR3_TEST_CASE(test_every_status_has_text), PR3_TEST_CASE(test_arena_allocations_are_aligned_and_disjoint),
    PR3_TEST_CASE(test_a_full_arena_returns_null_and_stays_usable),
    PR3_TEST_CASE(test_arena_size_overflow_is_refused),
    PR3_TEST_CASE(test_arena_rewind_releases_only_what_came_after_the_mark),
    PR3_TEST_CASE(test_an_empty_arena_hands_out_only_zero_bytes),
    PR3_TEST_CASE(test_released_arena_memory_is_poisoned),
    PR3_TEST_CASE(test_pool_handles_resolve_until_released),
    PR3_TEST_CASE(test_a_stale_handle_resolves_to_null_and_cannot_release),
    PR3_TEST_CASE(test_a_full_pool_refuses_and_recovers_after_a_release),
    PR3_TEST_CASE(test_acquired_slots_are_zeroed),
    PR3_TEST_CASE(test_a_slot_whose_generation_would_wrap_is_retired),
    PR3_TEST_CASE(test_pool_init_leaves_a_small_arena_unchanged),
    PR3_TEST_CASE(test_pool_bytes_covers_what_init_takes),
    PR3_TEST_CASE(test_buddy_blocks_are_powers_of_two_and_aligned),
    PR3_TEST_CASE(test_buddy_coalesces_back_to_one_block), PR3_TEST_CASE(test_a_full_buddy_heap_returns_null),
    PR3_TEST_CASE(test_buddy_random_traffic_never_overlaps_and_accounts_exactly),
    PR3_TEST_CASE(test_buddy_bytes_covers_what_init_takes),
    PR3_TEST_CASE(test_buddy_rejects_sizes_that_are_not_powers_of_two),
    PR3_TEST_CASE(test_ring_is_fifo_and_reports_full_and_empty), PR3_TEST_CASE(test_ring_indices_wrap),
    PR3_TEST_CASE(test_ring_rejects_bad_capacity), PR3_TEST_CASE(test_ring_transfers_across_threads),
    PR3_TEST_CASE(test_builder_overflow_is_sticky_and_reported_once),
    PR3_TEST_CASE(test_builder_writes_integers), PR3_TEST_CASE(test_trailing_zero_bits_are_counted),
    PR3_TEST_CASE(test_builder_writes_lowercase_hex),
    PR3_TEST_CASE(test_clip_cuts_on_a_character_boundary_with_the_marker),
    PR3_TEST_CASE(test_str_compare_is_byte_order),
    PR3_TEST_CASE(test_parse_u64_accepts_only_canonical_decimal),
    PR3_TEST_CASE(test_parse_u64_refuses_overflow),
    PR3_TEST_CASE(test_utf8_accepts_every_well_formed_sequence),
    PR3_TEST_CASE(test_utf8_rejects_overlongs_surrogates_and_out_of_range),
    PR3_TEST_CASE(test_utf8_rejects_truncated_sequences),
    PR3_TEST_CASE(test_utf8_encode_round_trips_every_scalar_value),
    PR3_TEST_CASE(test_utf8_validation_agrees_with_decoding_on_random_bytes),
    PR3_TEST_CASE(test_str_from_bytes_refuses_invalid_utf8),
    PR3_TEST_CASE(test_siphash24_matches_the_papers_example),
    PR3_TEST_CASE(test_siphash_matches_the_golden_vectors), PR3_TEST_CASE(test_vec_push_stops_at_capacity))
