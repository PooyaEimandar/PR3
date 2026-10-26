//! Messages between a phone and its computer, decoded from arbitrary bytes, as a
//! hostile peer could send them. Decoding never panics, and whatever decodes encodes
//! to bytes that decode to the same message.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pr3_protocol::{FromNode, ToNode, decode, encode};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn round_trips<T: Serialize + DeserializeOwned>(data: &[u8]) {
    if let Ok(message) = decode::<T>(data) {
        let bytes = encode(&message);
        let again: T = decode(&bytes).expect("a message's own encoding decodes");
        assert_eq!(encode(&again), bytes, "encoding is stable");
    }
}

fuzz_target!(|data: &[u8]| {
    round_trips::<ToNode>(data);
    round_trips::<FromNode>(data);
});
