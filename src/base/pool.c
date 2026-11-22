/* Pools of fixed-size slots behind generational handles. */
#include <pr3/base.h>

#include <pr3/poison.h>

static bool pr3_pool_slot_size(size_t slot_size, size_t align, size_t *out) {
    if (slot_size == 0 || align == 0 || (align & (align - 1)) != 0) return false;
    size_t rounded;
    if (pr3_ckd_add(&rounded, slot_size, align - 1)) return false;
    *out = rounded & ~(align - 1);
    return true;
}

size_t pr3_pool_bytes(size_t slot_size, size_t align, uint32_t cap) {
    size_t slot, slots, words, total;
    if (!pr3_pool_slot_size(slot_size, align, &slot)) return 0;
    if (pr3_ckd_mul(&slots, slot, cap) || pr3_ckd_mul(&words, sizeof(uint32_t), cap)) return 0;
    /* slots, then generations, then the free list, each with its worst-case padding
     * and, in asan builds, its red zone */
    size_t pad = align - 1 + (size_t)2 * (_Alignof(uint32_t) - 1) + (size_t)3 * PR3_ARENA_REDZONE +
                 (PR3_ASAN_POISON ? (size_t)3 * 7 : 0);
    if (pr3_ckd_add(&total, slots, words) || pr3_ckd_add(&total, total, words) ||
        pr3_ckd_add(&total, total, pad))
        return 0;
    return total;
}

pr3_status pr3_pool_init(pr3_pool *p, pr3_arena *from, size_t slot_size, size_t align, uint32_t cap) {
    size_t slot;
    if (!pr3_pool_slot_size(slot_size, align, &slot)) return PR3_E_INVALID;
    pr3_arena_mark mark = pr3_arena_mark_get(from);
    uint8_t *slots = pr3_arena_alloc_n(from, cap, slot, align);
    uint32_t *gens = PR3_NEW_N(from, uint32_t, cap);
    uint32_t *free_list = PR3_NEW_N(from, uint32_t, cap);
    if (slots == NULL || gens == NULL || free_list == NULL) {
        pr3_arena_rewind(from, mark);
        return PR3_E_NOMEM;
    }
    for (uint32_t i = 0; i < cap; i++) {
        gens[i] = 0;
        free_list[i] = cap - 1 - i; /* the first acquire takes slot 0 */
    }
    *p = (pr3_pool){
        .slots = slots,
        .gens = gens,
        .free_list = free_list,
        .slot_size = slot,
        .cap = cap,
        .n_free = cap,
        .live = 0,
        .high_water = 0,
    };
    PR3_POISON(slots, (size_t)cap * slot);
    return PR3_OK;
}

void *pr3_pool_acquire(pr3_pool *p, pr3_handle *out) {
    if (p->n_free == 0) return NULL;
    uint32_t index = p->free_list[--p->n_free];
    PR3_CHECK(index < p->cap && (p->gens[index] & 1u) == 0);
    uint32_t gen = p->gens[index] + 1u; /* odd: live; never wraps, see release */
    p->gens[index] = gen;
    p->live++;
    if (p->live > p->high_water) p->high_water = p->live;
    uint8_t *slot = p->slots + (size_t)index * p->slot_size;
    PR3_UNPOISON(slot, p->slot_size);
    memset(slot, 0, p->slot_size);
    *out = (pr3_handle){index, gen};
    return slot;
}

pr3_status pr3_pool_release(pr3_pool *p, pr3_handle h) {
    if (pr3_pool_get(p, h) == NULL) return PR3_E_STATE;
    uint8_t *slot = p->slots + (size_t)h.index * p->slot_size;
    PR3_RELEASE_FILL(slot, p->slot_size);
    PR3_POISON(slot, p->slot_size);
    p->live--;
    if (h.gen == UINT32_MAX) {
        /* The next generation would wrap to a value an old handle may hold: retire the
         * slot. Generation 0 matches no handle, and the slot never returns to the free
         * list. */
        p->gens[h.index] = 0;
        return PR3_OK;
    }
    p->gens[h.index] = h.gen + 1u; /* even: free */
    p->free_list[p->n_free++] = h.index;
    return PR3_OK;
}
