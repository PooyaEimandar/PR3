/* Single-producer single-consumer rings. Indices run freely and wrap; their difference
 * is the fill level. The producer and consumer each cache the other's index, so the
 * common case touches only their own cache line. */
#include <pr3/base.h>

#include <pr3/poison.h>

#if PR3_RING_FENCED_WORDS
static uint32_t load_relaxed(const pr3_ring_index *p) { return *p; }
static uint32_t load_acquire(const pr3_ring_index *p) {
    uint32_t v = *p;
    __atomic_thread_fence(__ATOMIC_ACQUIRE);
    return v;
}
static void store_release(pr3_ring_index *p, uint32_t v) {
    __atomic_thread_fence(__ATOMIC_RELEASE);
    *p = v;
}
static void store_init(pr3_ring_index *p, uint32_t v) { *p = v; }
#else
static uint32_t load_relaxed(pr3_ring_index *p) { return atomic_load_explicit(p, memory_order_relaxed); }
static uint32_t load_acquire(pr3_ring_index *p) { return atomic_load_explicit(p, memory_order_acquire); }
static void store_release(pr3_ring_index *p, uint32_t v) {
    atomic_store_explicit(p, v, memory_order_release);
}
static void store_init(pr3_ring_index *p, uint32_t v) { atomic_init(p, v); }
#endif

pr3_status pr3_ring_init(pr3_ring *r, pr3_arena *from, size_t slot_size, size_t align, uint32_t cap) {
    if (cap < 2 || (cap & (cap - 1)) != 0 || slot_size == 0 || align == 0 || (align & (align - 1)) != 0)
        return PR3_E_INVALID;
    size_t slot;
    if (pr3_ckd_add(&slot, slot_size, align - 1)) return PR3_E_INVALID;
    slot &= ~(align - 1);
    uint8_t *slots = pr3_arena_alloc_n(from, cap, slot, align);
    if (slots == NULL) return PR3_E_NOMEM;
    store_init(&r->head, 0u);
    store_init(&r->tail, 0u);
    r->tail_cache = 0;
    r->head_cache = 0;
    r->slots = slots;
    r->slot_size = slot;
    r->mask = cap - 1;
    return PR3_OK;
}

PR3_WRAPPING void *pr3_ring_reserve(pr3_ring *r) {
    uint32_t head = load_relaxed(&r->head);
    if (head - r->tail_cache > r->mask) {
        r->tail_cache = load_acquire(&r->tail);
        if (head - r->tail_cache > r->mask) return NULL;
    }
    return r->slots + (size_t)(head & r->mask) * r->slot_size;
}

PR3_WRAPPING void pr3_ring_publish(pr3_ring *r) {
    uint32_t head = load_relaxed(&r->head);
    PR3_DEBUG_ASSERT(head - r->tail_cache <= r->mask);
    store_release(&r->head, head + 1u);
}

PR3_WRAPPING void *pr3_ring_peek(pr3_ring *r) {
    uint32_t tail = load_relaxed(&r->tail);
    if (tail == r->head_cache) {
        r->head_cache = load_acquire(&r->head);
        if (tail == r->head_cache) return NULL;
    }
    return r->slots + (size_t)(tail & r->mask) * r->slot_size;
}

PR3_WRAPPING void pr3_ring_consume(pr3_ring *r) {
    uint32_t tail = load_relaxed(&r->tail);
    PR3_DEBUG_ASSERT(tail != r->head_cache);
    store_release(&r->tail, tail + 1u);
}
