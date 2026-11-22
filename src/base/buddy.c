/* The buddy heap. Free blocks are kept in one doubly linked list per order; the links
 * live in the first eight bytes of each free block, as indices in units of the smallest
 * block, so the heap needs one metadata byte per smallest block and nothing else. */
#include <pr3/base.h>

#include <pr3/poison.h>

#define MIN_BLOCK ((size_t)1 << PR3_BUDDY_MIN_ORDER)
#define NONE UINT32_MAX
#define META_FREE 0x80u  /* set on the first smallest block of a free block */
#define META_INNER 0x7Fu /* a smallest block that starts no block */

size_t pr3_buddy_block_size(size_t size) {
    size_t block = MIN_BLOCK;
    unsigned order = PR3_BUDDY_MIN_ORDER;
    while (block < size) {
        if (order == PR3_BUDDY_MAX_ORDER || order + 1 >= sizeof(size_t) * 8) return 0;
        block <<= 1;
        order++;
    }
    return block;
}

static unsigned pr3_buddy_order_of(size_t heap_bytes, bool *ok) {
    unsigned order = PR3_BUDDY_MIN_ORDER;
    *ok = false;
    if (heap_bytes < MIN_BLOCK || (heap_bytes & (heap_bytes - 1)) != 0) return 0;
    while (((size_t)1 << order) < heap_bytes) order++;
    if (order > PR3_BUDDY_MAX_ORDER) return 0;
    *ok = true;
    return order - PR3_BUDDY_MIN_ORDER;
}

size_t pr3_buddy_bytes(size_t heap_bytes) {
    bool ok;
    (void)pr3_buddy_order_of(heap_bytes, &ok);
    if (!ok) return 0;
    size_t total;
    size_t pad = MIN_BLOCK - 1 + (size_t)2 * PR3_ARENA_REDZONE + (PR3_ASAN_POISON ? 7u : 0u);
    if (pr3_ckd_add(&total, heap_bytes, heap_bytes / MIN_BLOCK) || pr3_ckd_add(&total, total, pad)) return 0;
    return total;
}

static uint8_t *block_at(const pr3_buddy *b, uint32_t idx) { return b->base + (size_t)idx * MIN_BLOCK; }

static void links_get(const pr3_buddy *b, uint32_t idx, uint32_t *next, uint32_t *prev) {
    uint8_t *p = block_at(b, idx);
    PR3_UNPOISON(p, 8);
    *next = pr3_load_le32(p);
    *prev = pr3_load_le32(p + 4);
    PR3_POISON(p, 8);
}

static void links_set(const pr3_buddy *b, uint32_t idx, uint32_t next, uint32_t prev) {
    uint8_t *p = block_at(b, idx);
    PR3_UNPOISON(p, 8);
    pr3_store_le32(p, next);
    pr3_store_le32(p + 4, prev);
    PR3_POISON(p, 8);
}

static void list_push(pr3_buddy *b, uint32_t k, uint32_t idx) {
    uint32_t head = b->free_head[k];
    links_set(b, idx, head, NONE);
    if (head != NONE) {
        uint32_t next, prev;
        links_get(b, head, &next, &prev);
        links_set(b, head, next, idx);
    }
    b->free_head[k] = idx;
    b->meta[idx] = (uint8_t)(META_FREE | k);
}

static void list_remove(pr3_buddy *b, uint32_t k, uint32_t idx) {
    uint32_t next, prev, n2, p2;
    links_get(b, idx, &next, &prev);
    if (prev != NONE) {
        links_get(b, prev, &n2, &p2);
        links_set(b, prev, next, p2);
    } else {
        b->free_head[k] = next;
    }
    if (next != NONE) {
        links_get(b, next, &n2, &p2);
        links_set(b, next, n2, prev);
    }
    b->meta[idx] = META_INNER;
}

pr3_status pr3_buddy_init(pr3_buddy *b, pr3_arena *from, size_t heap_bytes, const char *name) {
    bool ok;
    unsigned top = pr3_buddy_order_of(heap_bytes, &ok);
    if (!ok) return PR3_E_INVALID;
    size_t n_blocks = heap_bytes / MIN_BLOCK;
    pr3_arena_mark mark = pr3_arena_mark_get(from);
    uint8_t *base = pr3_arena_alloc(from, heap_bytes, MIN_BLOCK);
    uint8_t *meta = pr3_arena_alloc(from, n_blocks, 1);
    if (base == NULL || meta == NULL) {
        pr3_arena_rewind(from, mark);
        return PR3_E_NOMEM;
    }
    *b = (pr3_buddy){
        .base = base,
        .meta = meta,
        .size = heap_bytes,
        .used = 0,
        .high_water = 0,
        .top = top,
        .name = name,
    };
    for (size_t i = 0; i < PR3_BUDDY_ORDERS; i++) b->free_head[i] = NONE;
    memset(meta, META_INNER, n_blocks);
    PR3_POISON(base, heap_bytes);
    list_push(b, top, 0);
    return PR3_OK;
}

void *pr3_buddy_alloc(pr3_buddy *b, size_t size) {
    uint32_t k = 0;
    while ((MIN_BLOCK << k) < size) {
        if (k == b->top) return NULL;
        k++;
    }
    uint32_t j = k;
    while (b->free_head[j] == NONE) {
        if (j == b->top) return NULL;
        j++;
    }
    uint32_t idx = b->free_head[j];
    list_remove(b, j, idx);
    while (j > k) { /* split, keeping the lower half */
        j--;
        list_push(b, j, idx + ((uint32_t)1 << j));
    }
    b->meta[idx] = (uint8_t)k;
    b->used += MIN_BLOCK << k;
    if (b->used > b->high_water) b->high_water = b->used;
    uint8_t *p = block_at(b, idx);
    PR3_UNPOISON(p, size);
    return p;
}

void pr3_buddy_free(pr3_buddy *b, void *ptr) {
    if (ptr == NULL) return;
    uintptr_t at = (uintptr_t)ptr, lo = (uintptr_t)b->base;
    PR3_CHECK(at >= lo && at - lo < b->size && ((at - lo) & (MIN_BLOCK - 1)) == 0);
    uint32_t idx = (uint32_t)((at - lo) / MIN_BLOCK);
    uint32_t k = b->meta[idx];
    PR3_CHECK((k & META_FREE) == 0 && k <= b->top); /* a double release, or an inner pointer */
    size_t block = MIN_BLOCK << k;
    uint8_t *p = block_at(b, idx);
    PR3_UNPOISON(p, block);
    PR3_RELEASE_FILL(p, block);
    PR3_POISON(p, block);
    b->used -= block;
    while (k < b->top) { /* coalesce with the buddy while it is free and whole */
        uint32_t buddy = idx ^ ((uint32_t)1 << k);
        if (b->meta[buddy] != (META_FREE | k)) break;
        list_remove(b, k, buddy);
        b->meta[idx] = META_INNER;
        idx &= ~((uint32_t)1 << k);
        k++;
    }
    list_push(b, k, idx);
}
