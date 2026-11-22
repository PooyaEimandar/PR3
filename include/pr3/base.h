/* The base library: status codes, arenas, pools and handles, the buddy heap, SPSC
 * rings, slices and builders, UTF-8, decimal parsing and SipHash (doc/03).
 *
 * Conventions for every PR3 header: memory is the caller's; a function that can fail
 * returns pr3_status and writes its out-parameters only on success; nothing here
 * keeps global state, so every function is reentrant on distinct objects, and an
 * object is used by one thread at a time unless its section says otherwise. */
#ifndef PR3_BASE_H
#define PR3_BASE_H

#include <pr3/compiler.h>
#include <pr3/limits.h>

/* Ring indices are C11 atomics where the core has atomic instructions. Cores without them
 * (Cortex-M0+, RV32IMC) load and store an aligned word atomically anyway, so there the
 * indices are plain words ordered by fences, and no runtime helper is needed. */
#ifndef PR3_RING_FENCED_WORDS /* a build may choose; the CBMC harness does */
#if defined(__GCC_ATOMIC_INT_LOCK_FREE) && __GCC_ATOMIC_INT_LOCK_FREE < 2
#define PR3_RING_FENCED_WORDS 1
#else
#define PR3_RING_FENCED_WORDS 0
#endif
#endif
#if PR3_RING_FENCED_WORDS
typedef volatile uint32_t pr3_ring_index;
#else
#include <stdatomic.h>
typedef _Atomic uint32_t pr3_ring_index;
#endif

/* ---- Status codes */

typedef enum pr3_status {
    PR3_OK = 0,
    PR3_E_NOMEM,     /* an arena or pool is full */
    PR3_E_LIMIT,     /* input exceeds a configured limit */
    PR3_E_INVALID,   /* malformed input */
    PR3_E_UTF8,      /* invalid UTF-8 */
    PR3_E_STATE,     /* not allowed now (a turn in progress, a stale handle) */
    PR3_E_NOT_FOUND, /* no such item */
    PR3_E_IO,        /* the host's I/O failed */
    PR3_E_CRYPTO,    /* a signature, tag or key did not verify */
    PR3_E_CANCELLED, /* the operation was cancelled */
} pr3_status;

/* ---- Slices. A pr3_str is always valid UTF-8; raw bytes are pr3_bytes, and the two
 * never convert implicitly. Neither depends on a terminating NUL. */

typedef struct pr3_str {
    const char *ptr;
    size_t len;
} pr3_str;

typedef struct pr3_bytes {
    const uint8_t *ptr;
    size_t len;
} pr3_bytes;

/* A string literal as a pr3_str, without strlen. */
#define PR3_S(lit) ((pr3_str){"" lit, sizeof(lit) - 1})

/* The marker clipped text ends with. */
#define PR3_TRUNCATED_MARKER " \xe2\x80\xa6[truncated]"

PR3_INLINE bool pr3_str_eq(pr3_str a, pr3_str b) {
    return a.len == b.len && (a.len == 0 || memcmp(a.ptr, b.ptr, a.len) == 0);
}

/* A short English description, for logs and the model ("out of memory"). */
pr3_str pr3_status_text(pr3_status s);

/* pr3_str_cmp past a shared first byte. */
int pr3_str_cmp_rest(pr3_str a, pr3_str b);

/* Byte order: negative, zero or positive, like memcmp, with a shorter prefix first. Keys
 * and names mostly differ in their first byte, which is settled here without a call. */
PR3_INLINE int pr3_str_cmp(pr3_str a, pr3_str b) {
    if (a.len != 0 && b.len != 0 && a.ptr[0] != b.ptr[0])
        return (int)(uint8_t)a.ptr[0] - (int)(uint8_t)b.ptr[0];
    return pr3_str_cmp_rest(a, b);
}

PR3_INLINE pr3_bytes pr3_str_bytes(pr3_str s) { return (pr3_bytes){(const uint8_t *)s.ptr, s.len}; }

/* Validates b and views it as text. PR3_E_UTF8 if it is not valid UTF-8. */
PR3_NODISCARD pr3_status pr3_str_from_bytes(pr3_bytes b, pr3_str *out);

/* The longest prefix of s of at most max bytes that ends on a character boundary.
 * The one place text is cut; pr3_buf_put_clip adds the marker. */
pr3_str pr3_str_clip(pr3_str s, size_t max);

/* A canonical unsigned decimal: one or more ASCII digits, no sign, no leading zero
 * unless the value is 0. PR3_E_INVALID if malformed, PR3_E_LIMIT if over UINT64_MAX. */
PR3_NODISCARD pr3_status pr3_parse_u64(pr3_str s, uint64_t *out);

/* ---- UTF-8 (RFC 3629: no overlong forms, no surrogates, nothing above U+10FFFF). */

/* The length of the longest valid prefix of p[0, n); n when all of it is valid. */
size_t pr3_utf8_valid_prefix(const uint8_t *p, size_t n);

PR3_NODISCARD PR3_INLINE pr3_status pr3_utf8_validate(const uint8_t *p, size_t n) {
    return pr3_utf8_valid_prefix(p, n) == n ? PR3_OK : PR3_E_UTF8;
}

/* Decodes one scalar value at p (n > 0 bytes available) into *cp and returns its
 * length, 1 to 4; returns 0, leaving *cp alone, if the bytes there are not valid. */
size_t pr3_utf8_decode(const uint8_t *p, size_t n, uint32_t *cp);

/* Encodes cp into out and returns its length, 1 to 4; 0 if cp is a surrogate or above
 * U+10FFFF. */
size_t pr3_utf8_encode(uint32_t cp, uint8_t out[4]);

/* ---- Arenas (doc/03, "Arenas"). A bump allocator over one block the host owns.
 * Exhaustion returns NULL. In asan builds released memory is poisoned and each
 * allocation is followed by a poisoned red zone; debug builds fill released memory
 * with 0xDD. */

typedef struct pr3_arena {
    uint8_t *base;     /* first byte of the block */
    size_t cap;        /* bytes in the block */
    size_t used;       /* bytes handed out, including alignment padding */
    size_t high_water; /* largest `used` ever seen, for budgets */
    const char *name;  /* for diagnostics and budget reports */
} pr3_arena;

typedef struct pr3_arena_mark {
    size_t used;
} pr3_arena_mark;

/* Bytes each allocation may cost beyond its size and alignment: the red zone in asan
 * builds, nothing otherwise. Sizing formulas add it per allocation. */
#define PR3_ARENA_REDZONE (PR3_ASAN_POISON ? 16u : 0u)

/* mem may be NULL only with cap 0. */
void pr3_arena_init(pr3_arena *a, void *mem, size_t cap, const char *name);

/* Hands the block back to the host: unpoisons it, so the host may reuse or free it.
 * The arena must not be used again until it is initialized again. */
void pr3_arena_fini(pr3_arena *a);

void *pr3_arena_alloc_slow(pr3_arena *a, size_t size, size_t align);

/* size bytes aligned to align (a power of two), or NULL when the arena is full. A
 * zero-byte request returns a valid pointer that must not be dereferenced. */
PR3_INLINE void *pr3_arena_alloc(pr3_arena *a, size_t size, size_t align) {
#if PR3_ASAN_POISON
    return pr3_arena_alloc_slow(a, size, align);
#else
    PR3_DEBUG_ASSERT(align != 0 && (align & (align - 1)) == 0);
    size_t mis = (size_t)(((uintptr_t)a->base + a->used) & (uintptr_t)(align - 1));
    size_t start, end;
    if (PR3_UNLIKELY(pr3_ckd_add(&start, a->used, (align - mis) & (align - 1)) ||
                     pr3_ckd_add(&end, start, size) || end > a->cap))
        return NULL;
    a->used = end;
    if (end > a->high_water) a->high_water = end;
    return a->base + start;
#endif
}

/* n elements of size bytes; NULL on overflow or when full. */
PR3_INLINE void *pr3_arena_alloc_n(pr3_arena *a, size_t n, size_t size, size_t align) {
    size_t total;
    if (PR3_UNLIKELY(pr3_ckd_mul(&total, n, size))) return NULL;
    return pr3_arena_alloc(a, total, align);
}

PR3_INLINE pr3_arena_mark pr3_arena_mark_get(const pr3_arena *a) { return (pr3_arena_mark){a->used}; }

/* Releases everything allocated after m was taken. */
void pr3_arena_rewind(pr3_arena *a, pr3_arena_mark m);

/* Releases everything. */
void pr3_arena_reset(pr3_arena *a);

PR3_INLINE size_t pr3_arena_remaining(const pr3_arena *a) { return a->cap - a->used; }

#define PR3_NEW(a, T) ((T *)pr3_arena_alloc((a), sizeof(T), _Alignof(T)))
#define PR3_NEW_N(a, T, n) ((T *)pr3_arena_alloc_n((a), (n), sizeof(T), _Alignof(T)))

/* ---- Pools and generational handles (doc/03, "Pools and handles"). Fixed-size slots
 * for objects whose lifetimes do not nest. A released slot's generation moves on, so a
 * stale handle resolves to NULL. A slot is retired, never reused, when its generation
 * would wrap, so a stale handle can never match again. */

typedef struct pr3_handle {
    uint32_t index;
    uint32_t gen; /* 0 is the null handle; a live slot's generation is odd */
} pr3_handle;

#define PR3_HANDLE_NULL ((pr3_handle){0, 0})

PR3_INLINE bool pr3_handle_is_null(pr3_handle h) { return h.gen == 0; }
PR3_INLINE bool pr3_handle_eq(pr3_handle a, pr3_handle b) { return a.index == b.index && a.gen == b.gen; }

typedef struct pr3_pool {
    uint8_t *slots;
    uint32_t *gens;      /* per slot: odd while live, even while free */
    uint32_t *free_list; /* stack of free slot indices */
    size_t slot_size;    /* rounded up to the alignment */
    uint32_t cap;
    uint32_t n_free;
    uint32_t live;
    uint32_t high_water; /* most slots ever live at once */
} pr3_pool;

/* Arena bytes pr3_pool_init takes, alignment padding included; 0 on overflow. */
size_t pr3_pool_bytes(size_t slot_size, size_t align, uint32_t cap);

/* Takes the slots, generations and free list from `from`. PR3_E_INVALID for a zero
 * slot size or an alignment that is not a power of two; PR3_E_NOMEM, with the arena
 * unchanged, when it is too small. */
PR3_NODISCARD pr3_status pr3_pool_init(pr3_pool *p, pr3_arena *from, size_t slot_size, size_t align,
                                       uint32_t cap);

/* A zeroed slot and its handle, or NULL (and *out untouched) when the pool is full. */
void *pr3_pool_acquire(pr3_pool *p, pr3_handle *out);

/* The slot h names, or NULL when h is null, out of range or stale. */
PR3_INLINE void *pr3_pool_get(const pr3_pool *p, pr3_handle h) {
    if (PR3_UNLIKELY(h.index >= p->cap || p->gens[h.index] != h.gen || (h.gen & 1u) == 0)) return NULL;
    return p->slots + (size_t)h.index * p->slot_size;
}

/* PR3_E_STATE when h is null, out of range or stale. */
PR3_NODISCARD pr3_status pr3_pool_release(pr3_pool *p, pr3_handle h);

/* ---- The buddy heap (doc/04, "The history heap"). A power-of-two region split into
 * blocks of 2^k bytes from 2^PR3_BUDDY_MIN_ORDER up, split on allocation and coalesced
 * with the buddy on release, both in O(log n). Free blocks are poisoned in asan
 * builds. */

#define PR3_BUDDY_ORDERS (PR3_BUDDY_MAX_ORDER - PR3_BUDDY_MIN_ORDER + 1u)

typedef struct pr3_buddy {
    uint8_t *base;
    uint8_t *meta;     /* per smallest block: the order of the block that starts there */
    size_t size;       /* bytes in the region, a power of two */
    size_t used;       /* bytes in allocated blocks */
    size_t high_water; /* largest `used` ever seen */
    uint32_t top;      /* the region's order, counted from the smallest block */
    uint32_t free_head[PR3_BUDDY_ORDERS];
    const char *name;
} pr3_buddy;

/* The block a request of size bytes occupies; 0 if no heap could hold it. */
size_t pr3_buddy_block_size(size_t size);

/* Arena bytes pr3_buddy_init takes for a heap of heap_bytes; 0 if heap_bytes is not a
 * power of two between the smallest block and 2^PR3_BUDDY_MAX_ORDER. */
size_t pr3_buddy_bytes(size_t heap_bytes);

PR3_NODISCARD pr3_status pr3_buddy_init(pr3_buddy *b, pr3_arena *from, size_t heap_bytes, const char *name);

/* size bytes aligned to 64, or NULL when no block that large is free. */
void *pr3_buddy_alloc(pr3_buddy *b, size_t size);

/* Releases a block pr3_buddy_alloc returned; NULL is ignored. A pointer the heap did
 * not hand out, or a second release, is a broken invariant and traps. */
void pr3_buddy_free(pr3_buddy *b, void *p);

/* ---- Rings (doc/03, "Rings"). Single producer, single consumer, fixed slots, power-of-
 * two capacity, no locks. A slot belongs to the producer from reserve to publish and to
 * the consumer from peek to consume. Exactly one thread produces and one consumes. */

typedef struct pr3_ring {
    _Alignas(64) pr3_ring_index head; /* written by the producer */
    uint32_t tail_cache;              /* the producer's last view of tail */
    _Alignas(64) pr3_ring_index tail; /* written by the consumer */
    uint32_t head_cache;              /* the consumer's last view of head */
    _Alignas(64) uint8_t *slots;
    size_t slot_size;
    uint32_t mask;
} pr3_ring;

/* PR3_E_INVALID unless cap is a power of two of at least 2 and slot_size is non-zero;
 * PR3_E_NOMEM, with the arena unchanged, when it is too small. */
PR3_NODISCARD pr3_status pr3_ring_init(pr3_ring *r, pr3_arena *from, size_t slot_size, size_t align,
                                       uint32_t cap);

/* Producer: the next free slot, or NULL when the ring is full (back-pressure). */
void *pr3_ring_reserve(pr3_ring *r);
/* Producer: hands the reserved slot to the consumer. */
void pr3_ring_publish(pr3_ring *r);
/* Consumer: the oldest published slot, or NULL when the ring is empty. */
void *pr3_ring_peek(pr3_ring *r);
/* Consumer: hands the peeked slot back to the producer. */
void pr3_ring_consume(pr3_ring *r);

/* ---- Builders (doc/03, "Strings and builders"). A full builder sets `overflow` and
 * ignores further writes; the caller checks once, at pr3_buf_take. */

typedef struct pr3_buf {
    char *ptr;
    size_t len;
    size_t cap;
    bool overflow;
} pr3_buf;

PR3_INLINE pr3_buf pr3_buf_make(char *mem, size_t cap) { return (pr3_buf){mem, 0, mem ? cap : 0, false}; }

/* A builder over cap bytes of a; PR3_E_NOMEM, with the arena unchanged, when full. */
PR3_NODISCARD pr3_status pr3_buf_from_arena(pr3_arena *a, size_t cap, pr3_buf *out);

/* n > 0 bytes the caller writes in place, already counted in len; NULL, with overflow
 * set, if they do not fit. */
PR3_INLINE char *pr3_buf_reserve(pr3_buf *b, size_t n) {
    PR3_DEBUG_ASSERT(n > 0);
    if (PR3_UNLIKELY(b->overflow || n > b->cap - b->len)) {
        b->overflow = true;
        return NULL;
    }
    char *p = b->ptr + b->len;
    b->len += n;
    return p;
}

/* Raw bytes; the caller vouches that the text stays valid UTF-8. */
PR3_INLINE void pr3_buf_put_bytes(pr3_buf *b, const void *p, size_t n) {
    if (n == 0) return;
    char *dst = pr3_buf_reserve(b, n);
    if (PR3_LIKELY(dst != NULL)) memcpy(dst, p, n);
}

PR3_INLINE void pr3_buf_put(pr3_buf *b, pr3_str s) { pr3_buf_put_bytes(b, s.ptr, s.len); }

PR3_INLINE void pr3_buf_put_char(pr3_buf *b, char c) {
    char *dst = pr3_buf_reserve(b, 1);
    if (PR3_LIKELY(dst != NULL)) *dst = c;
}

/* Decimal digits, written in place. */
PR3_INLINE void pr3_buf_put_u64(pr3_buf *b, uint64_t v) {
    size_t n = 1;
    for (uint64_t t = v; t >= 10; t /= 10) n++;
    char *dst = pr3_buf_reserve(b, n);
    if (PR3_UNLIKELY(dst == NULL)) return;
    do {
        dst[--n] = (char)('0' + (int)(v % 10));
        v /= 10;
    } while (v != 0);
}

PR3_INLINE void pr3_buf_put_i64(pr3_buf *b, int64_t v) {
    if (v < 0) {
        pr3_buf_put_char(b, '-');
        pr3_buf_put_u64(b, (uint64_t)(-(v + 1)) + 1u); /* INT64_MIN without overflow */
    } else {
        pr3_buf_put_u64(b, (uint64_t)v);
    }
}
/* Lowercase hexadecimal, two digits per byte. */
void pr3_buf_put_hex(pr3_buf *b, pr3_bytes bytes);
/* s, or its first max bytes cut on a character boundary followed by the marker. */
void pr3_buf_put_clip(pr3_buf *b, pr3_str s, size_t max);

/* The built text, valid until the builder's memory is released. PR3_E_LIMIT if
 * anything did not fit. */
PR3_NODISCARD PR3_INLINE pr3_status pr3_buf_take(const pr3_buf *b, pr3_str *out) {
    if (b->overflow) return PR3_E_LIMIT;
    *out = (pr3_str){b->ptr, b->len};
    return PR3_OK;
}

/* ---- Fixed-capacity lists. Capacity comes from an arena; pushing at capacity is
 * PR3_E_LIMIT. */

#define PR3_VEC(T)                                                                                           \
    struct {                                                                                                 \
        T *items;                                                                                            \
        uint32_t len;                                                                                        \
        uint32_t cap;                                                                                        \
    }

#define PR3_VEC_INIT(a, v, T, n)                                                                             \
    ((v)->len = 0, (v)->items = PR3_NEW_N((a), T, (n)), (v)->cap = (v)->items ? (uint32_t)(n) : 0u,          \
     (v)->items ? PR3_OK : PR3_E_NOMEM)

#define PR3_VEC_PUSH(v, x)                                                                                   \
    ((v)->len < (v)->cap ? ((v)->items[(v)->len] = (x), (v)->len += 1u, PR3_OK) : PR3_E_LIMIT)

/* ---- SipHash (Aumasson and Bernstein). SipHash-1-3 keys PR3's hash tables and the
 * loop guard; SipHash-2-4 is kept for its published test vectors. */

typedef struct pr3_sipkey {
    uint64_t k0, k1;
} pr3_sipkey;

PR3_INLINE pr3_sipkey pr3_sipkey_from_bytes(const uint8_t key[16]) {
    return (pr3_sipkey){pr3_load_le64(key), pr3_load_le64(key + 8)};
}

typedef struct pr3_siphash {
    uint64_t v0, v1, v2, v3;
    uint64_t tail;    /* pending bytes, little-endian */
    uint64_t total;   /* bytes absorbed */
    uint8_t n_tail;   /* pending byte count, 0 to 7 */
    uint8_t c_rounds; /* compression rounds */
    uint8_t d_rounds; /* finalization rounds */
} pr3_siphash;

void pr3_siphash_init(pr3_siphash *h, pr3_sipkey key, unsigned c_rounds, unsigned d_rounds);
void pr3_siphash_update(pr3_siphash *h, const void *data, size_t len);
uint64_t pr3_siphash_final(const pr3_siphash *h);

uint64_t pr3_siphash13(pr3_sipkey key, const void *data, size_t len);
uint64_t pr3_siphash24(pr3_sipkey key, const void *data, size_t len);

#endif /* PR3_BASE_H */
