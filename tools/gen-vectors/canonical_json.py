"""Canonical JSON vectors (doc/04, "Canonical JSON"), from the rules alone.

Python's own parser reads the input; the hooks below impose PR3's rules on what it
returns, and the writer below lays the result out canonically. Python's float() is
correctly rounded and repr() gives the shortest digits that read back, so both sides
of every number vector come from the standard library, not from PR3's code.

Files:
  json/canonical.jsonl  {"in": text, "out": canonical}            plus "eq" when the
                        comparison form (-0.0 written as 0.0) differs from "out"
  json/numbers.jsonl    {"in": number, "kind": "u64"|"i64"|"f64", "out": canonical}
                        plus "bits" (the double, 16 hex digits, big-endian) for f64
  json/invalid.jsonl    {"in": text | "in_hex": bytes, "error": "invalid"|"utf8"|"limit"}
"""

import decimal
import json
import random
import struct

MAX_DEPTH = 127
U64_MAX = (1 << 64) - 1
I64_MIN = -(1 << 63)


class Refused(Exception):
    def __init__(self, error):
        super().__init__(error)
        self.error = error


# ---- reading


def _int(text):
    if text == "-0":
        return -0.0
    n = int(text)
    if 0 <= n <= U64_MAX or I64_MIN <= n < 0:
        return n
    return _float(text)


def _float(text):
    v = float(text)
    if v in (float("inf"), float("-inf")):
        raise Refused("invalid")  # out of the range of a double
    return v


def _constant(_name):
    raise Refused("invalid")  # NaN, Infinity, -Infinity are not JSON


def _pairs(pairs):
    obj = {}
    for k, v in pairs:
        obj[k] = v  # the last duplicate wins
    return obj


def _depth(v):
    if isinstance(v, (list, dict)):
        children = v if isinstance(v, list) else v.values()
        return 1 + max((_depth(c) for c in children), default=0)
    return 0


def _check_strings(v):
    """Lone surrogates arrive as escapes Python accepts; PR3 refuses them."""
    if isinstance(v, str):
        if any(0xD800 <= ord(c) <= 0xDFFF for c in v):
            raise Refused("invalid")
    elif isinstance(v, list):
        for c in v:
            _check_strings(c)
    elif isinstance(v, dict):
        for k, c in v.items():
            _check_strings(k)
            _check_strings(c)


def parse(data):
    """The value PR3 reads from `data` (bytes), or Refused with its error class."""
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        raise Refused("utf8") from None
    try:
        v = json.loads(
            text, parse_int=_int, parse_float=_float, parse_constant=_constant, object_pairs_hook=_pairs
        )
    except json.JSONDecodeError:
        raise Refused("invalid") from None
    except RecursionError:
        raise Refused("limit") from None
    if _depth(v) > MAX_DEPTH:
        raise Refused("limit")
    _check_strings(v)
    return v


# ---- writing

_ESC = {'"': '\\"', "\\": "\\\\", "\b": "\\b", "\f": "\\f", "\n": "\\n", "\r": "\\r", "\t": "\\t"}


def put_string(s):
    out = ['"']
    for c in s:
        if c in _ESC:
            out.append(_ESC[c])
        elif ord(c) < 0x20:
            out.append("\\u%04x" % ord(c))
        else:
            out.append(c)
    out.append('"')
    return "".join(out)


def put_double(v):
    """doc/04's layout of the shortest digits: decimal from 1e-5 up to below 1e16."""
    sign = "-" if struct.pack(">d", v)[0] & 0x80 else ""
    if v == 0.0:
        return sign + "0.0"
    r = repr(abs(v))
    mant, _, exp = r.partition("e")
    whole, _, frac = mant.partition(".")
    digits = whole + frac
    point = len(whole) + (int(exp) if exp else 0)  # value = 0.digits * 10^point after the strip
    stripped = digits.lstrip("0")
    point -= len(digits) - len(stripped)
    digits = stripped.rstrip("0")
    n, kk = len(digits), point
    if n <= kk <= 16:
        body = digits + "0" * (kk - n) + ".0"
    elif 0 < kk <= 16:
        body = digits[:kk] + "." + digits[kk:]
    elif -5 < kk <= 0:
        body = "0." + "0" * -kk + digits
    elif n == 1:
        body = digits + "e" + str(kk - 1)
    else:
        body = digits[0] + "." + digits[1:] + "e" + str(kk - 1)
    return sign + body


def write(v, zero_unsigned=False):
    if v is None:
        return "null"
    if v is True:
        return "true"
    if v is False:
        return "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        return put_double(0.0 if zero_unsigned and v == 0.0 else v)
    if isinstance(v, str):
        return put_string(v)
    if isinstance(v, list):
        return "[" + ",".join(write(c, zero_unsigned) for c in v) + "]"
    keys = sorted(v, key=lambda k: k.encode("utf-8"))
    return "{" + ",".join(put_string(k) + ":" + write(v[k], zero_unsigned) for k in keys) + "}"


def canonical(data):
    return write(parse(data))


# ---- canonical.jsonl

CANONICAL_INPUTS = [
    # scalars and whitespace
    "null", "true", "false", " \t\r\n null \t\r\n ", "0", "-1", '""', "[]", "{}", "[ ]", "{ }",
    # integers stay integers; everything else is a double
    "[0,1,-1,18446744073709551615,-9223372036854775808]",
    "[18446744073709551616,-9223372036854775809]",
    "[-0,-0.0,0.0,0e5,-0e-5]", "[1.0,1e0,10e-1,0.1e1]", "[100,100.0,1e2,1E2,1e+2]",
    "[1e15,1e16,1e17,123456789012345678]", "[0.0001,0.00001,0.000001,1.5e-7]",
    "[9007199254740993,9007199254740993.0,1e-400000000000000000000000000000000001,0e99999999999999999999]",
    "[1e-400,-1e-400,4.9e-324,2.4e-324,2.5e-324]",
    # key order is byte order; the last duplicate wins
    '{"b":1,"a":2,"c":3}', '{"a":1,"a":2}', '{"a":{"b":1,"b":[2]},"a":{"c":3}}',
    '{"B":1,"a":2,"_":3,"0":4,"":5}', '{"é":1,"e":2,"\U0001f600":3,"￿":4,"z":5}',
    '{"ab":1,"a":2,"abc":3,"b":4}',
    '{"k9":9,"k1":1,"k8":8,"k2":2,"k7":7,"k3":3,"k6":6,"k4":4,"k5":5,"k0":0,"k1":11,"k5":55}',
    # escapes in and out
    r'"\"\\\/\b\f\n\r\t"', r'"\u0000\u0001\u001f \u007f\u0080"', r'"é€😀"',
    '"é€\U0001f600  "', r'"😀é"', r'{"\n":"\t","A":"A"}',
    '"\x7f"', r'"a\u0000b"',
    # nesting
    "[[[[[[[[[[]]]]]]]]]]", '{"a":[{"b":[{"c":[]}]}]}', "[" * MAX_DEPTH + "]" * MAX_DEPTH,
    '{"a":' * MAX_DEPTH + "1" + "}" * MAX_DEPTH,
    # shapes tool calls take
    '{"name":"get_weather","arguments":{"city":"Tehran","unit":"celsius","days":3}}',
    '{"query":"café مرحبا","limit":10,"filters":{"lang":["fa","en"],'
    '"since":null},"score":0.75}',
]


def _random_value(rng, depth):
    pick = rng.randrange(10 if depth < 6 else 6)
    if pick == 0:
        return None
    if pick == 1:
        return rng.random() < 0.5
    if pick == 2:
        return rng.choice([0, 1, -1, rng.getrandbits(64), -rng.getrandbits(63) - 1, rng.randrange(-999, 999)])
    if pick == 3:
        bits = rng.getrandbits(64)
        v = struct.unpack(">d", struct.pack(">Q", bits))[0]
        return v if v == v and v not in (float("inf"), float("-inf")) else 0.5
    if pick in (4, 5):
        alphabet = 'abcXYZ09 _"\\/\b\f\n\r\t\x00\x1f\x7fé€ \U0001f600'
        return "".join(rng.choice(alphabet) for _ in range(rng.randrange(8)))
    if pick in (6, 7):
        return [_random_value(rng, depth + 1) for _ in range(rng.randrange(5))]
    keys = ["a", "b", "aa", "B", "", "é", "k" + str(rng.randrange(20))]
    return [(rng.choice(keys), _random_value(rng, depth + 1)) for _ in range(rng.randrange(12))]


def _random_text(rng, v):
    """Any JSON text for v, with random whitespace and random key order kept as given."""
    ws = lambda: rng.choice(["", "", " ", "\n", "\t ", "\r\n"])  # noqa: E731
    if isinstance(v, list) and v and isinstance(v[0], tuple):
        return "{" + ws() + ",".join(
            ws() + json.dumps(k, ensure_ascii=rng.random() < 0.5) + ws() + ":" + ws() + _random_text(rng, c) + ws()
            for k, c in v
        ) + "}"
    if isinstance(v, list) and v == []:
        return rng.choice(["[]", "{}"])
    if isinstance(v, list):
        return "[" + ",".join(ws() + _random_text(rng, c) + ws() for c in v) + "]"
    if isinstance(v, float):
        return rng.choice([repr(v), "%.17g" % v, "%.20e" % v]).replace("inf", "1e999")
    return json.dumps(v, ensure_ascii=rng.random() < 0.5)


def canonical_vectors():
    texts = list(CANONICAL_INPUTS)
    rng = random.Random(0x5052334A)  # fixed: the file is reproducible
    for _ in range(400):
        texts.append(_random_text(rng, _random_value(rng, 0)))
    rows = []
    for text in texts:
        data = text.encode("utf-8")
        v = parse(data)  # every canonical input must be accepted
        row = {"in": text, "out": write(v)}
        eq = write(v, zero_unsigned=True)
        if eq != row["out"]:
            row["eq"] = eq
        rows.append(row)
    return rows


# ---- numbers.jsonl


def _bits(v):
    return struct.pack(">d", v).hex()


def _number_row(text):
    v = parse(text.encode("ascii"))
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        raise ValueError(text)
    if isinstance(v, int):
        return {"in": text, "kind": "u64" if v >= 0 else "i64", "out": str(v)}
    return {"in": text, "kind": "f64", "out": put_double(v), "bits": _bits(v)}


def _exact(v):
    """The exact decimal expansion of a double, as a JSON number."""
    return format(decimal.Decimal(v), "f")


def _midpoint(bits):
    """The exact decimal halfway between the double with these bits and the next one up."""
    a = decimal.Decimal(struct.unpack(">d", struct.pack(">Q", bits))[0])
    b = decimal.Decimal(struct.unpack(">d", struct.pack(">Q", bits + 1))[0])
    with decimal.localcontext() as ctx:
        ctx.prec = 2000
        return format((a + b) / 2, "f")


NUMBER_INPUTS = [
    # integers at the edges of the two kinds
    "0", "1", "-1", "9", "10", "4294967295", "4294967296", "9223372036854775807", "9223372036854775808",
    "18446744073709551615", "-9223372036854775807", "-9223372036854775808",
    # just past them: doubles
    "18446744073709551616", "-9223372036854775809", "99999999999999999999", "-0",
    # the layout's boundaries
    "1e-5", "0.00001", "9.999999999999999e-6", "1e-6", "1e15", "9999999999999998.0", "1e16",
    "1.2345e16", "123456789012345.6", "0.1", "0.2", "0.3", "1.5", "-2.5", "3.141592653589793",
    "1.7976931348623157e308", "-1.7976931348623157e308", "2.2250738585072014e-308",
    "2.2250738585072011e-308", "4.9406564584124654e-324", "5e-324", "2.4703282292062328e-324",
    "2.4703282292062327e-324", "1e-400", "1.7976931348623158e308",
    # the fast path's boundaries: 2^53 and 10^22
    "9007199254740992", "9007199254740993.0", "9007199254740994.0", "9007199254740995.0",
    "1e22", "1e23", "123456789e15", "123456789e-15", "9007199254740991e22", "9007199254740993e22",
    # hard cases for correct rounding
    "2.2250738585072012e-308", "0.1000000000000000055511151231257827021181583404541015625",
    "0.1000000000000000055511151231257827021181583404541015624",
    "0.1000000000000000055511151231257827021181583404541015626",
    "1.00000000000000011102230246251565404236316680908203125",
    "1.00000000000000011102230246251565404236316680908203124",
    "1.00000000000000011102230246251565404236316680908203126",
    "7.3177701707893310e15", "8.9255e-18", "4.4501477170144023e-308", "1e308", "1e-307",
    "0." + "0" * 340 + "1", "1" + "0" * 400 + "e-400", "1" + "0" * 1000 + "e-1000", "1" + "0" * 1000 + "1e-1001",
    "9" * 900 + "e-900", "0e400", "-0e-400", "0.0",
]


def number_vectors():
    texts = list(NUMBER_INPUTS)
    rng = random.Random(0x4E554D53)
    for _ in range(600):  # random doubles in their shortest and longer forms
        bits = rng.getrandbits(64)
        v = struct.unpack(">d", struct.pack(">Q", bits))[0]
        if v != v or v in (float("inf"), float("-inf")):
            continue
        texts.append(rng.choice([repr(v), "%.17g" % v, "%.25e" % v, "%.40g" % v]))
    for _ in range(150):  # exact expansions and exact halfway points: correct rounding's hardest
        bits = rng.getrandbits(63)
        if (bits >> 52) >= 0x7FE:
            continue
        v = struct.unpack(">d", struct.pack(">Q", bits))[0]
        mid = _midpoint(bits)
        texts.append(_exact(v))
        texts.append(mid)
        texts.append(mid + "0000000000000000000000001")
        texts.append(mid[:-1] + str(int(mid[-1]) - 1) + "9" * 30 if mid[-1] != "0" else mid)
    for e in range(-325, 310, 7):  # every decade, both notations
        texts.append("1e%d" % e)
        texts.append("-7.5e%d" % e)
    rows, seen = [], set()
    for text in texts:
        if text in seen:
            continue
        seen.add(text)
        try:
            rows.append(_number_row(text))
        except Refused:
            continue  # out of range: invalid.jsonl covers those
    return rows


# ---- invalid.jsonl

INVALID_TEXTS = [
    # not JSON
    "", " ", "nul", "nulls", "True", "NaN", "Infinity", "-Infinity", "[1,]", "[,1]", "[1 2]", "{,}",
    '{"a"}', '{"a":}', '{"a":1,}', '{a:1}', "{'a':1}", '{"a" 1}', "[", "]", "{", "}", "[}", "{]",
    "[1]]", "1 2", '"abc', '"\\"', '"\\x"', '"\\u12"', '"\\u12G4"', '"\\U0041"', '"a\tb"', '"a\nb"',
    '"\x00"', '"\x1f"', "﻿1", "/*c*/1", "1 // c",
    # numbers
    "01", "-01", "+1", "-", "1.", ".1", "1.e1", "1e", "1e+", "1e-", "0x10", "1_000", "--1", "1.0.0",
    "-.5", "Inf", "1e309", "-1e309", "1.7976931348623159e308", "123e999999999999999999",
    "[1e309]", '{"a":-1e400}',
    # lone and misordered surrogates
    '"\\ud800"', '"\\udc00"', '"\\ud800x"', '"\\ud800\\u0041"', '"\\udc00\\ud800"', '"\\ud83d\\ud83d"',
    '{"\\ud800":1}',
    # one past the nesting limit, otherwise well formed
    "[" * (MAX_DEPTH + 1) + "]" * (MAX_DEPTH + 1),
    '{"a":' * (MAX_DEPTH + 1) + "1" + "}" * (MAX_DEPTH + 1),
    "[" * 64 + '{"a":' * 64 + "1" + "}" * 64 + "]" * 64,
]

INVALID_BYTES = [
    b"\xff", b'"\xc0\xaf"', b'"\xe0\x80\xaf"', b'"\xed\xa0\x80"', b'"\xf4\x90\x80\x80"', b'"\x80"',
    b'"\xe2\x82"', b'"\xf0\x9f\x98"', b"[1,\xfe]", b'{"\xc3":1}', b"\xc3\xa9", b"1\xa0",
]


def invalid_vectors():
    rows = []
    for text in INVALID_TEXTS:
        rows.append(_refused({"in": text}, text.encode("utf-8")))
    for data in INVALID_BYTES:
        rows.append(_refused({"in_hex": data.hex()}, data))
    return rows


def _refused(row, data):
    try:
        parse(data)
    except Refused as r:
        row["error"] = r.error
        return row
    raise ValueError("accepted: %r" % data)


FILES = [
    ("json/canonical.jsonl", "canonical-json", 1, canonical_vectors),
    ("json/numbers.jsonl", "canonical-json-numbers", 1, number_vectors),
    ("json/invalid.jsonl", "canonical-json-invalid", 1, invalid_vectors),
]
