"""SipHash written from the paper ("SipHash: a fast short-input PRF", Aumasson and
Bernstein, 2012), independently of src/base/siphash.c."""

MASK = (1 << 64) - 1


def _rotl(x, b):
    return ((x << b) | (x >> (64 - b))) & MASK


def siphash(key, msg, c, d):
    k0 = int.from_bytes(key[:8], "little")
    k1 = int.from_bytes(key[8:16], "little")
    v = [k0 ^ 0x736F6D6570736575, k1 ^ 0x646F72616E646F6D, k0 ^ 0x6C7967656E657261, k1 ^ 0x7465646279746573]

    def sipround():
        v[0] = (v[0] + v[1]) & MASK
        v[1] = _rotl(v[1], 13) ^ v[0]
        v[0] = _rotl(v[0], 32)
        v[2] = (v[2] + v[3]) & MASK
        v[3] = _rotl(v[3], 16) ^ v[2]
        v[0] = (v[0] + v[3]) & MASK
        v[3] = _rotl(v[3], 21) ^ v[0]
        v[2] = (v[2] + v[1]) & MASK
        v[1] = _rotl(v[1], 17) ^ v[2]
        v[2] = _rotl(v[2], 32)

    def absorb(m):
        v[3] ^= m
        for _ in range(c):
            sipround()
        v[0] ^= m

    whole = len(msg) - len(msg) % 8
    for i in range(0, whole, 8):
        absorb(int.from_bytes(msg[i : i + 8], "little"))
    absorb(int.from_bytes(msg[whole:], "little") | (len(msg) & 0xFF) << 56)
    v[2] ^= 0xFF
    for _ in range(d):
        sipround()
    return v[0] ^ v[1] ^ v[2] ^ v[3]


def self_check():
    """The two values the paper prints, so this oracle is checked against the source."""
    key = bytes(range(16))
    assert siphash(key, bytes(range(15)), 2, 4) == 0xA129CA6149BE45E5
    assert siphash(key, b"", 2, 4) == 0x726FDB47DD0E0E31


def vectors():
    """SipHash-2-4 and SipHash-1-3 of the messages 00, 00 01, ... up to 63 bytes under the
    key 00 01 ... 0f: the shape of the reference implementation's vector tables."""
    self_check()
    key = bytes(range(16))
    for c, d in ((2, 4), (1, 3)):
        for n in range(64):
            msg = bytes(range(n))
            out = siphash(key, msg, c, d).to_bytes(8, "little")
            yield {"c": c, "d": d, "key": key.hex(), "msg": msg.hex(), "out": out.hex()}
