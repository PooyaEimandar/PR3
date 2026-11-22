/* Arenas: the out-of-line parts. The allocation fast path is inline in pr3/base.h. */
#include <pr3/base.h>

#include <pr3/poison.h>

/* What an empty arena points at, so no arithmetic is ever done on NULL. */
static uint8_t pr3_arena_empty[1];

void pr3_arena_init(pr3_arena *a, void *mem, size_t cap, const char *name) {
    PR3_CHECK(mem != NULL || cap == 0);
    *a = (pr3_arena){
        .base = mem ? (uint8_t *)mem : pr3_arena_empty,
        .cap = mem ? cap : 0,
        .used = 0,
        .high_water = 0,
        .name = name,
    };
    PR3_POISON(a->base, a->cap);
}

void pr3_arena_fini(pr3_arena *a) {
    PR3_UNPOISON(a->base, a->cap);
    a->used = a->cap; /* any further allocation fails */
}

/* The asan path: every allocation starts on an 8-byte granule and is followed by a
 * poisoned red zone, so an overrun into the next allocation is reported. */
void *pr3_arena_alloc_slow(pr3_arena *a, size_t size, size_t align) {
    PR3_CHECK(align != 0 && (align & (align - 1)) == 0);
    if (align < 8) align = 8;
    size_t mis = (size_t)(((uintptr_t)a->base + a->used) & (uintptr_t)(align - 1));
    size_t start, end, next;
    if (pr3_ckd_add(&start, a->used, (align - mis) & (align - 1)) || pr3_ckd_add(&end, start, size) ||
        end > a->cap)
        return NULL;
    if (pr3_ckd_add(&next, end, PR3_ARENA_REDZONE) || next > a->cap) next = a->cap;
    a->used = next;
    if (next > a->high_water) a->high_water = next;
    PR3_UNPOISON(a->base + start, size);
    return a->base + start;
}

void pr3_arena_rewind(pr3_arena *a, pr3_arena_mark m) {
    PR3_CHECK(m.used <= a->used);
    PR3_RELEASE_FILL(a->base + m.used, a->used - m.used);
    PR3_POISON(a->base + m.used, a->used - m.used);
    a->used = m.used;
}

void pr3_arena_reset(pr3_arena *a) { pr3_arena_rewind(a, (pr3_arena_mark){0}); }
