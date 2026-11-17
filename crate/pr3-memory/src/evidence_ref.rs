//! Compact integrity references for untrusted source excerpts. These are not
//! capabilities: the caller must first authorize the source and check deletion.
use alloc::{format, string::String};
use pr3_core::trusted::Hash;

/// Bind a bounded byte range to both its source identity and exact text. Opaque
/// hexadecimal references discourage interpreting offsets as editable arguments.
pub fn quote_reference(source: &str, offset: u64, text: &str) -> Option<String> {
    let length = u32::try_from(text.len()).ok()?;
    if !(1..=500).contains(&length) {
        return None;
    }
    let packed = u32::try_from(offset)
        .ok()?
        .checked_mul(512)?
        .checked_add(length)?;
    let hash = Hash::of(&[
        b"pr3:quote-reference:v1",
        source.as_bytes(),
        &packed.to_be_bytes(),
        text.as_bytes(),
    ])
    .to_hex();
    Some(format!("q:{packed:08x}{}", hash.get(..16)?))
}

pub fn resolve_quote_reference<'a>(
    source: &str,
    reference: &str,
    text: &'a str,
) -> Option<&'a str> {
    let reference_bytes = reference.strip_prefix("q:")?;
    if !(17..=24).contains(&reference_bytes.len()) {
        return None;
    }
    // Models sometimes omit leading zero padding from the numeric range. Its
    // value is bound into the checksum, so accept that representation only;
    // the complete checksum and the resolved source/text must still match.
    let checksum_start = reference_bytes.len() - 16;
    let range = reference_bytes.get(..checksum_start)?;
    if !range.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let packed = u32::from_str_radix(range, 16).ok()?;
    let offset = usize::try_from(packed >> 9).ok()?;
    let length = usize::try_from(packed & 511).ok()?;
    let quote = text.get(offset..offset.checked_add(length)?)?;
    let expected = quote_reference(source, offset as u64, quote)?;
    (expected.get(10..)? == reference_bytes.get(checksum_start..)?).then_some(quote)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binds_the_source_range_and_exact_bytes_without_granting_access() {
        let source = "s:0123456789abcdef";
        let text = "Intro. The fee is 27 euros. Other text.";
        let quote = "The fee is 27 euros.";
        let reference = quote_reference(source, 7, quote).unwrap();
        assert_eq!(reference.len(), 26);
        assert_eq!(
            resolve_quote_reference(source, &reference, text),
            Some(quote)
        );
        assert!(resolve_quote_reference("s:foreign", &reference, text).is_none());
        assert!(
            resolve_quote_reference(
                source,
                &reference,
                "Intro. The fee is 99 euros. Other text."
            )
            .is_none()
        );
        let mut tampered = reference.clone();
        tampered.replace_range(2..10, "00000001");
        assert!(resolve_quote_reference(source, &tampered, text).is_none());
        assert!(resolve_quote_reference(source, "q:7:20", text).is_none());
        assert!(quote_reference(source, u64::MAX, quote).is_none());
        assert!(quote_reference(source, 0, &"x".repeat(501)).is_none());
        let unicode = quote_reference(source, 0, "é").unwrap();
        assert_eq!(resolve_quote_reference(source, &unicode, "é"), Some("é"));
        let split = quote_reference(source, 1, "é").unwrap();
        assert!(resolve_quote_reference(source, &split, "é").is_none());
    }

    #[test]
    fn permits_missing_range_padding_but_never_missing_checksum_or_changed_range() {
        let source = "s:245ef49028e5e7bc";
        let text = "Brisa cuesta 27 euros al mes.";
        let reference = quote_reference(source, 0, text).unwrap();
        assert_eq!(reference, "q:0000001d1a73c1a3fd4b72cb");
        for short in ["q:00001d1a73c1a3fd4b72cb", "q:1d1a73c1a3fd4b72cb"] {
            assert_eq!(resolve_quote_reference(source, short, text), Some(text));
            assert!(resolve_quote_reference("s:another", short, text).is_none());
            assert!(
                resolve_quote_reference(source, short, "Brisa cuesta 99 euros al mes.").is_none()
            );
        }
        for malformed in [
            "q:d1a73c1a3fd4b72cb",         // Missing a nonzero range digit.
            "q:1d1a73c1a3fd4b72c",         // Missing a checksum digit.
            "q:1d1a73c1a3fd4b72cc",        // Changed checksum.
            "q:00000001d1a73c1a3fd4b72cb", // More than eight range digits.
            "q:+1d1a73c1a3fd4b72cb",       // Non-hex numeric syntax.
            "q:1a73c1a3fd4b72cb",          // No range.
            "q:01a73c1a3fd4b72cb",         // Empty quote.
            "q:1ff1a73c1a3fd4b72cb",       // Quote exceeds 500 bytes.
        ] {
            assert!(
                resolve_quote_reference(source, malformed, text).is_none(),
                "{malformed}"
            );
        }
    }
}
