/* Compiler portability: attributes, branch hints, checked arithmetic, traps and
 * sanitizer hooks. C17 is the floor; C23 features are used where the compiler has
 * them (doc/19 D-05). Every PR3 header includes this one first. */
#ifndef PR3_COMPILER_H
#define PR3_COMPILER_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* The portable libraries need only the compiler's own headers and these five functions
 * (doc/11, "The board contract"). A freestanding build has no <string.h>, so the board's
 * C runtime or compiler-rt supplies them. */
#if defined(__STDC_HOSTED__) && __STDC_HOSTED__
#include <string.h>
#else
void *memcpy(void *dst, const void *src, size_t n);
void *memmove(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
int memcmp(const void *a, const void *b, size_t n);
void *memchr(const void *s, int c, size_t n);
#endif

#if defined(__has_attribute)
#define PR3_HAS_ATTRIBUTE(x) __has_attribute(x)
#else
#define PR3_HAS_ATTRIBUTE(x) 0
#endif

#if defined(__has_builtin)
#define PR3_HAS_BUILTIN(x) __has_builtin(x)
#else
#define PR3_HAS_BUILTIN(x) 0
#endif

#if defined(__has_feature)
#define PR3_HAS_FEATURE(x) __has_feature(x)
#else
#define PR3_HAS_FEATURE(x) 0
#endif

/* A status that is ignored is a compile error under the warning set. */
#if PR3_HAS_ATTRIBUTE(warn_unused_result)
#define PR3_NODISCARD __attribute__((warn_unused_result))
#elif defined(_MSC_VER)
#define PR3_NODISCARD _Check_return_
#else
#define PR3_NODISCARD
#endif

/* A trailing array whose length is another member, for the bounds sanitizer and
 * __builtin_dynamic_object_size. */
#if PR3_HAS_ATTRIBUTE(counted_by)
#define PR3_COUNTED_BY(member) __attribute__((counted_by(member)))
#else
#define PR3_COUNTED_BY(member)
#endif

#if PR3_HAS_BUILTIN(__builtin_expect)
#define PR3_LIKELY(x) __builtin_expect(!!(x), 1)
#define PR3_UNLIKELY(x) __builtin_expect(!!(x), 0)
#else
#define PR3_LIKELY(x) (x)
#define PR3_UNLIKELY(x) (x)
#endif

#if PR3_HAS_ATTRIBUTE(always_inline)
#define PR3_INLINE static inline __attribute__((always_inline))
#else
#define PR3_INLINE static inline
#endif

#if PR3_HAS_ATTRIBUTE(noinline)
#define PR3_NOINLINE __attribute__((noinline))
#elif defined(_MSC_VER)
#define PR3_NOINLINE __declspec(noinline)
#else
#define PR3_NOINLINE
#endif

#if PR3_HAS_ATTRIBUTE(fallthrough)
#define PR3_FALLTHROUGH __attribute__((fallthrough))
#else
#define PR3_FALLTHROUGH ((void)0)
#endif

/* Functions whose arithmetic is modular on purpose (hashes, ring indices). The
 * ubsan preset checks unsigned overflow and shifted-out bits everywhere else. */
#if defined(__clang__)
#define PR3_WRAPPING __attribute__((no_sanitize("unsigned-integer-overflow", "unsigned-shift-base")))
#else
#define PR3_WRAPPING
#endif

/* ---- Checked arithmetic: true means the result overflowed and *r is unspecified. */

#if PR3_HAS_BUILTIN(__builtin_add_overflow) || (defined(__GNUC__) && __GNUC__ >= 5)
PR3_INLINE bool pr3_ckd_add(size_t *r, size_t a, size_t b) { return __builtin_add_overflow(a, b, r); }
PR3_INLINE bool pr3_ckd_mul(size_t *r, size_t a, size_t b) { return __builtin_mul_overflow(a, b, r); }
PR3_INLINE bool pr3_ckd_add_u64(uint64_t *r, uint64_t a, uint64_t b) {
    return __builtin_add_overflow(a, b, r);
}
PR3_INLINE bool pr3_ckd_mul_u64(uint64_t *r, uint64_t a, uint64_t b) {
    return __builtin_mul_overflow(a, b, r);
}
#else
PR3_INLINE bool pr3_ckd_add(size_t *r, size_t a, size_t b) {
    if (a > SIZE_MAX - b) return true;
    *r = a + b;
    return false;
}
PR3_INLINE bool pr3_ckd_mul(size_t *r, size_t a, size_t b) {
    if (a != 0 && b > SIZE_MAX / a) return true;
    *r = a * b;
    return false;
}
PR3_INLINE bool pr3_ckd_add_u64(uint64_t *r, uint64_t a, uint64_t b) {
    if (a > UINT64_MAX - b) return true;
    *r = a + b;
    return false;
}
PR3_INLINE bool pr3_ckd_mul_u64(uint64_t *r, uint64_t a, uint64_t b) {
    if (a != 0 && b > UINT64_MAX / a) return true;
    *r = a * b;
    return false;
}
#endif

/* ---- Invariants. PR3_CHECK guards a broken internal invariant, never input; it is
 * kept in release builds. tools/check-traps.sh lists every site. */

#if PR3_HAS_BUILTIN(__builtin_trap) || defined(__GNUC__)
#define PR3_TRAP() __builtin_trap()
#elif defined(_MSC_VER)
#define PR3_TRAP() __fastfail(7)
#else
#define PR3_TRAP() (*(volatile int *)0 = 0)
#endif

#define PR3_CHECK(cond)                                                                                      \
    do {                                                                                                     \
        if (PR3_UNLIKELY(!(cond))) PR3_TRAP();                                                               \
    } while (0)

#if defined(NDEBUG)
#define PR3_DEBUG_ASSERT(cond) ((void)0)
#else
#define PR3_DEBUG_ASSERT(cond) PR3_CHECK(cond)
#endif

#define PR3_STATIC_ASSERT(cond, msg) _Static_assert(cond, msg)
#define PR3_ARRAY_LEN(a) (sizeof(a) / sizeof((a)[0]))

/* ---- Sanitizer hooks. PR3_ASAN_POISON is set by the asan preset; debug builds
 * without it fill released memory with 0xDD so stale reads stand out. */

#if defined(PR3_ASAN_POISON) && PR3_ASAN_POISON
#include <sanitizer/asan_interface.h>
#define PR3_POISON(p, n) ASAN_POISON_MEMORY_REGION((p), (n))
#define PR3_UNPOISON(p, n) ASAN_UNPOISON_MEMORY_REGION((p), (n))
#define PR3_RELEASE_FILL(p, n) ((void)(p), (void)(n))
#else
#undef PR3_ASAN_POISON
#define PR3_ASAN_POISON 0
#define PR3_POISON(p, n) ((void)(p), (void)(n))
#define PR3_UNPOISON(p, n) ((void)(p), (void)(n))
#if defined(NDEBUG)
#define PR3_RELEASE_FILL(p, n) ((void)(p), (void)(n))
#else
#define PR3_RELEASE_FILL(p, n) memset((p), 0xDD, (n))
#endif
#endif

/* ---- Unaligned little-endian loads and stores; each compiles to one instruction. */

PR3_INLINE uint32_t pr3_load_le32(const void *p) {
    const uint8_t *b = (const uint8_t *)p;
    return (uint32_t)b[0] | (uint32_t)b[1] << 8 | (uint32_t)b[2] << 16 | (uint32_t)b[3] << 24;
}

PR3_INLINE uint64_t pr3_load_le64(const void *p) {
    return (uint64_t)pr3_load_le32(p) | (uint64_t)pr3_load_le32((const uint8_t *)p + 4) << 32;
}

/* The masks make each narrowing exact, so conversion checkers (CBMC, -fsanitize=implicit-
 * conversion) see no lost bits; the compiler folds them away. */
PR3_INLINE void pr3_store_le32(void *p, uint32_t v) {
    uint8_t *b = (uint8_t *)p;
    b[0] = (uint8_t)(v & 0xFFu);
    b[1] = (uint8_t)((v >> 8) & 0xFFu);
    b[2] = (uint8_t)((v >> 16) & 0xFFu);
    b[3] = (uint8_t)(v >> 24);
}

PR3_INLINE void pr3_store_le64(void *p, uint64_t v) {
    pr3_store_le32(p, (uint32_t)(v & 0xFFFFFFFFu));
    pr3_store_le32((uint8_t *)p + 4, (uint32_t)(v >> 32));
}

/* The number of trailing zero bits of x, which is not zero. */
PR3_INLINE unsigned pr3_ctz64(uint64_t x) {
#if PR3_HAS_BUILTIN(__builtin_ctzll) || defined(__GNUC__)
    return (unsigned)__builtin_ctzll(x);
#else
    unsigned n = 0;
    while ((x & 1u) == 0) {
        x >>= 1;
        n++;
    }
    return n;
#endif
}

PR3_INLINE uint64_t pr3_rotl64(uint64_t x, unsigned b) {
#if PR3_HAS_BUILTIN(__builtin_rotateleft64)
    return __builtin_rotateleft64(x, b);
#else
    return (x << (b & 63u)) | (x >> ((64u - b) & 63u));
#endif
}

#endif /* PR3_COMPILER_H */
