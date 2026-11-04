//! Reading a page the user linked, under arbitrary pages and messages (docs RFC-002): a
//! page always becomes parts no larger than the model is given, however it nests, and
//! the links found in a message are `https` addresses that normalize to themselves, so
//! the link the model names and the one the user wrote compare as equals.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use pr3_tool::web;

#[derive(Arbitrary, Debug)]
struct Case {
    page: String,
    said: String,
    part_bytes: u16,
}

fuzz_target!(|case: Case| {
    let (title, text) = web::page_text(&case.page);
    if let Some(title) = title {
        assert!(!title.is_empty() && title.chars().count() <= 200, "{title:?}");
    }
    let max = usize::from(case.part_bytes).max(4);
    for part in web::parts(&text, max) {
        assert!(!part.is_empty() && part.len() <= max, "{part:?}");
    }
    for link in web::links(&case.said) {
        assert!(link.starts_with("https://"), "{link}");
        assert_eq!(web::normalize(&link).as_deref(), Some(link.as_str()));
    }
});
