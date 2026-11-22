/* SipHash, after "SipHash: a fast short-input PRF" (Aumasson and Bernstein, 2012),
 * with the number of compression and finalization rounds as parameters. */
#include <pr3/base.h>

#include <pr3/poison.h>

PR3_WRAPPING static void sipround(pr3_siphash *h) {
    h->v0 += h->v1;
    h->v1 = pr3_rotl64(h->v1, 13);
    h->v1 ^= h->v0;
    h->v0 = pr3_rotl64(h->v0, 32);
    h->v2 += h->v3;
    h->v3 = pr3_rotl64(h->v3, 16);
    h->v3 ^= h->v2;
    h->v0 += h->v3;
    h->v3 = pr3_rotl64(h->v3, 21);
    h->v3 ^= h->v0;
    h->v2 += h->v1;
    h->v1 = pr3_rotl64(h->v1, 17);
    h->v1 ^= h->v2;
    h->v2 = pr3_rotl64(h->v2, 32);
}

static void absorb(pr3_siphash *h, uint64_t m) {
    h->v3 ^= m;
    for (unsigned i = 0; i < h->c_rounds; i++) sipround(h);
    h->v0 ^= m;
}

void pr3_siphash_init(pr3_siphash *h, pr3_sipkey key, unsigned c_rounds, unsigned d_rounds) {
    *h = (pr3_siphash){
        .v0 = key.k0 ^ UINT64_C(0x736f6d6570736575),
        .v1 = key.k1 ^ UINT64_C(0x646f72616e646f6d),
        .v2 = key.k0 ^ UINT64_C(0x6c7967656e657261),
        .v3 = key.k1 ^ UINT64_C(0x7465646279746573),
        .tail = 0,
        .total = 0,
        .n_tail = 0,
        .c_rounds = (uint8_t)c_rounds,
        .d_rounds = (uint8_t)d_rounds,
    };
}

PR3_WRAPPING void pr3_siphash_update(pr3_siphash *h, const void *data, size_t len) {
    const uint8_t *p = (const uint8_t *)data;
    h->total += len;
    while (len > 0 && h->n_tail != 0) { /* top up a pending word */
        h->tail |= (uint64_t)*p++ << (8u * h->n_tail);
        len--;
        if (++h->n_tail == 8) {
            absorb(h, h->tail);
            h->tail = 0;
            h->n_tail = 0;
        }
    }
    while (len >= 8) {
        absorb(h, pr3_load_le64(p));
        p += 8;
        len -= 8;
    }
    while (len > 0) {
        h->tail |= (uint64_t)*p++ << (8u * h->n_tail);
        h->n_tail++;
        len--;
    }
}

PR3_WRAPPING uint64_t pr3_siphash_final(const pr3_siphash *state) {
    pr3_siphash h = *state;
    absorb(&h, h.tail | (h.total & 0xFF) << 56);
    h.v2 ^= 0xFF;
    for (unsigned i = 0; i < h.d_rounds; i++) sipround(&h);
    return h.v0 ^ h.v1 ^ h.v2 ^ h.v3;
}

uint64_t pr3_siphash13(pr3_sipkey key, const void *data, size_t len) {
    pr3_siphash h;
    pr3_siphash_init(&h, key, 1, 3);
    pr3_siphash_update(&h, data, len);
    return pr3_siphash_final(&h);
}

uint64_t pr3_siphash24(pr3_sipkey key, const void *data, size_t len) {
    pr3_siphash h;
    pr3_siphash_init(&h, key, 2, 4);
    pr3_siphash_update(&h, data, len);
    return pr3_siphash_final(&h);
}
