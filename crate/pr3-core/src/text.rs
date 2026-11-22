//! Writing text into buffers the core keeps, so steady-state turns allocate nothing.

use alloc::string::String;

/// The marker a cut text ends with.
pub(crate) const CUT: &str = " …[truncated]";

/// Appends `text` cut to at most `max` bytes on a character boundary, marking the cut.
pub(crate) fn push_clipped(out: &mut String, text: &str, max: usize) {
    if text.len() <= max {
        out.push_str(text);
        return;
    }
    let mut cut = max;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    out.push_str(text.get(..cut).unwrap_or(""));
    out.push_str(CUT);
}

/// Every two-digit number, for writing decimals two digits at a time.
const PAIRS: &[u8; 200] = b"0001020304050607080910111213141516171819202122232425262728293031323334353637383940414243444546474849\
    5051525354555657585960616263646566676869707172737475767778798081828384858687888990919293949596979899";

/// Appends `n` in decimal, as `core::fmt` and `serde_json` write it, without either.
/// The digits are pushed as ASCII characters, so there is nothing to validate.
pub(crate) fn push_u64(out: &mut String, mut n: u64) {
    let ascii = |b: u8| char::from(b & 0x7F);
    if n < 10 {
        out.push(ascii(b'0' + n as u8));
        return;
    }
    let mut digits = [b'0'; 20];
    let mut at = digits.len();
    let mut put = |at: usize, pair: u64| {
        let pair = pair as usize * 2;
        if let (Some(d), Some(p)) = (digits.get_mut(at..at + 2), PAIRS.get(pair..pair + 2)) {
            d.copy_from_slice(p);
        }
    };
    while n >= 100 {
        at -= 2;
        put(at, n % 100);
        n /= 100;
    }
    at -= 2;
    put(at, n);
    if n < 10 {
        at += 1;
    }
    let digits = digits.get(at..).unwrap_or(&[]);
    out.reserve(digits.len());
    for &d in digits {
        out.push(ascii(d));
    }
}

pub(crate) fn push_i64(out: &mut String, n: i64) {
    if n < 0 {
        out.push('-');
    }
    push_u64(out, n.unsigned_abs());
}

/// Appends `text` with every run of whitespace turned into one space, trimmed.
pub(crate) fn push_one_line(out: &mut String, text: &str) {
    for (i, word) in text.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(word);
    }
}

const ONES: u64 = 0x0101_0101_0101_0101;
const HIGHS: u64 = 0x8080_8080_8080_8080;

/// The high bit of each byte of `w` (little-endian) that JSON escapes: a control byte,
/// `"` or `\`. A bit above the lowest one may be a false alarm (a borrow from the byte
/// below); the lowest is always exact, and only the lowest is used.
fn escape_flags(w: u64) -> u64 {
    let control = w.wrapping_sub(ONES * 0x20) & !w;
    let quote = w ^ (ONES * u64::from(b'"'));
    let backslash = w ^ (ONES * u64::from(b'\\'));
    let quote = quote.wrapping_sub(ONES) & !quote;
    let backslash = backslash.wrapping_sub(ONES) & !backslash;
    (control | quote | backslash) & HIGHS
}

/// The first byte at or after `from` that JSON escapes, eight bytes at a time.
pub(crate) fn next_escape(bytes: &[u8], mut from: usize) -> Option<usize> {
    loop {
        let rest = bytes.get(from..)?;
        let word = match rest.first_chunk::<8>() {
            Some(chunk) => *chunk,
            None => {
                // The tail, padded with a byte that needs no escape.
                let mut padded = [b'a'; 8];
                padded.get_mut(..rest.len())?.copy_from_slice(rest);
                padded
            }
        };
        let flags = escape_flags(u64::from_le_bytes(word));
        if flags != 0 {
            return Some(from + (flags.trailing_zeros() / 8) as usize);
        }
        if rest.len() <= 8 {
            return None;
        }
        from += 8;
    }
}

fn push_escape(out: &mut String, b: u8) {
    let short = match b {
        b'"' => "\\\"",
        b'\\' => "\\\\",
        0x08 => "\\b",
        0x0C => "\\f",
        b'\n' => "\\n",
        b'\r' => "\\r",
        b'\t' => "\\t",
        _ => {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            out.push_str("\\u00");
            for d in [b >> 4, b & 0xF] {
                out.push(char::from(HEX.get(usize::from(d)).copied().unwrap_or(b'0')));
            }
            return;
        }
    };
    out.push_str(short);
}

/// Appends `text` as a JSON string, escaped exactly as `serde_json` writes it.
pub(crate) fn push_json_str(out: &mut String, text: &str) {
    out.reserve(text.len() + 2);
    out.push('"');
    let bytes = text.as_bytes();
    let mut start = 0;
    // Every byte found is ASCII, so each run between them is whole characters.
    while let Some(at) = next_escape(bytes, start) {
        out.push_str(text.get(start..at).unwrap_or(""));
        push_escape(out, bytes.get(at).copied().unwrap_or(0));
        start = at + 1;
    }
    out.push_str(text.get(start..).unwrap_or(""));
    out.push('"');
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped_as_serde_json_writes_them() {
        for text in [
            "",
            "plain",
            "q\"b\\s",
            "\u{8}\u{c}\n\r\t",
            "\u{1}\u{1f}\u{7f}",
            "é/€😀",
        ] {
            let mut out = String::new();
            push_json_str(&mut out, text);
            assert_eq!(out, serde_json::to_string(text).unwrap(), "{text:?}");
        }
    }

    /// Every escaped byte at every offset of the eight-byte scan, between multi-byte
    /// characters and runs of plain text, and next to bytes one away from escaping.
    #[test]
    fn the_word_scan_escapes_every_byte_at_every_offset() {
        let specials = ['"', '\\', '\u{0}', '\u{8}', '\n', '\u{1f}'];
        let near = ['!', '#', '[', ']', ' ', '\u{7f}', 'é', '€', '😀'];
        for before in 0..20 {
            for after in [0, 1, 7, 8, 9] {
                for (k, &special) in specials.iter().enumerate() {
                    let fill = near[(before + k) % near.len()];
                    let mut text: String = core::iter::repeat_n(fill, before).collect();
                    text.push(special);
                    text.extend(core::iter::repeat_n(
                        specials[(k + after) % specials.len()],
                        after % 3,
                    ));
                    text.extend(core::iter::repeat_n('x', after));
                    let mut out = String::from("keep");
                    push_json_str(&mut out, &text);
                    let want = alloc::format!("keep{}", serde_json::to_string(&text).unwrap());
                    assert_eq!(out, want, "{text:?}");
                }
            }
        }
    }

    #[test]
    fn numbers_are_written_as_core_fmt_writes_them() {
        for n in [0, 7, 10, 99, 100, 101, 12_345, 1_000_000, u64::MAX] {
            let mut out = String::new();
            push_u64(&mut out, n);
            assert_eq!(out, alloc::format!("{n}"));
        }
        for n in [0, -1, 42, i64::MIN, i64::MAX] {
            let mut out = String::new();
            push_i64(&mut out, n);
            assert_eq!(out, alloc::format!("{n}"));
        }
    }

    #[test]
    fn a_cut_lands_on_a_character_boundary() {
        let mut out = String::new();
        push_clipped(&mut out, "aé", 2);
        assert_eq!(out, "a …[truncated]");
    }
}
