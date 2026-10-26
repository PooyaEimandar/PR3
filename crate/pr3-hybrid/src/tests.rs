//! Pairing, sessions, revocation and cancellation over real sockets on localhost.

use std::io::Write as _;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pr3_core::{GenerationInfo, InferenceRequest, Message, RequestId, Role, SessionId, Timestamp};
use pr3_protocol::Generated;

use super::*;

/// Answers with the question's words, one token each, slowly enough to cancel.
struct Echo {
    calls: Mutex<u32>,
}

impl Generator for Echo {
    fn model(&self) -> String {
        "echo".to_string()
    }

    fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str) -> bool,
    ) -> Result<Generated, String> {
        *self.calls.lock().unwrap() += 1;
        let question = request.messages.last().map_or("", |m| m.content.as_str());
        let mut said = String::new();
        for word in question.split_inclusive(' ') {
            std::thread::sleep(Duration::from_millis(20));
            if !on_token(word) {
                return Err("stopped".to_string());
            }
            said.push_str(word);
        }
        Ok(Generated {
            content: said,
            tool_calls: Vec::new(),
            info: GenerationInfo::default(),
        })
    }
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pr3-hybrid-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn request(text: &str) -> InferenceRequest {
    InferenceRequest {
        id: RequestId(1),
        session: SessionId(1),
        messages: vec![Message::new(Role::User, text.to_string(), Timestamp(1))],
        tools: Vec::new(),
    }
}

/// A node on a free localhost port, and its address.
fn node(name: &str) -> (Arc<Node>, String, PathBuf) {
    let dir = temp(name);
    let generator = Arc::new(Echo {
        calls: Mutex::new(0),
    });
    let node = Node::open(&dir.join("node"), "Test Mac", generator).unwrap();
    let address = node.serve("127.0.0.1:0").unwrap().to_string();
    (node, address, dir)
}

#[test]
fn a_paired_phone_gets_answers_streamed_from_the_node() {
    let (node, address, dir) = node("pair");
    let phone_key = DeviceKey::open(&dir.join("phone/device.key")).unwrap();
    let code = node.start_pairing().unwrap().code;
    let paired = pair(&address, &code, &phone_key, "Test phone").unwrap();
    assert_eq!(paired.name, "Test Mac");
    assert_eq!(paired.address.as_deref(), Some(address.as_str()));
    let phones = node.phones();
    assert_eq!(phones.len(), 1);
    assert_eq!(phones[0].name, "Test phone");
    assert_eq!(phones[0].public_key, phone_key.public_hex());

    let client = NodeClient::new(phone_key, paired, "Test phone");
    assert_eq!(client.model().unwrap(), "echo");
    let mut streamed = String::new();
    let answer = client
        .generate(
            &request("hello from the phone"),
            &mut |t| streamed.push_str(t),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(answer.content, "hello from the phone");
    assert_eq!(streamed, answer.content);

    // The node's list survives a restart.
    let reopened = Node::open(
        &dir.join("node"),
        "Test Mac",
        Arc::new(Echo {
            calls: Mutex::new(0),
        }),
    );
    assert_eq!(reopened.unwrap().phones().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_code_works_once_wrong_codes_fail_and_nothing_pairs_without_one() {
    let (node, address, dir) = node("codes");
    let key = DeviceKey::open(&dir.join("phone/device.key")).unwrap();

    let refused = pair(&address, "123456", &key, "phone").unwrap_err();
    assert!(
        matches!(
            refused,
            HybridError::Refused(_) | HybridError::Unreachable(_)
        ),
        "{refused}"
    );

    let code = node.start_pairing().unwrap().code;
    let wrong = if code == "000000" { "000001" } else { "000000" };
    let refused = pair(&address, wrong, &key, "phone").unwrap_err();
    assert!(
        refused.to_string().contains("wrong") || matches!(refused, HybridError::Unreachable(_)),
        "{refused}"
    );
    // The right code no longer works either: one attempt per code.
    assert!(pair(&address, &code, &key, "phone").is_err());
    assert!(node.phones().is_empty());
    assert!(
        pair(&address, "12345", &key, "phone").is_err(),
        "codes have six digits"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unknown_and_forgotten_phones_are_refused() {
    let (node, address, dir) = node("revoke");
    let key = DeviceKey::open(&dir.join("phone/device.key")).unwrap();
    let code = node.start_pairing().unwrap().code;
    let paired = pair(&address, &code, &key, "phone").unwrap();

    // Another device that claims to be paired is turned away.
    let stranger = NodeClient::new(DeviceKey::generate().unwrap(), paired.clone(), "stranger");
    assert!(matches!(stranger.model(), Err(HybridError::Refused(_))));

    let client = NodeClient::new(key, paired.clone(), "phone");
    assert!(client.model().is_ok());
    assert!(node.forget(&paired_phone_id(&node)).unwrap());
    assert!(matches!(client.model(), Err(HybridError::Refused(_))));
    let _ = std::fs::remove_dir_all(dir);
}

fn paired_phone_id(node: &Node) -> String {
    node.phones()[0].id()
}

#[test]
fn cancelling_stops_the_node_mid_answer() {
    let (node, address, dir) = node("cancel");
    let key = DeviceKey::open(&dir.join("phone/device.key")).unwrap();
    let code = node.start_pairing().unwrap().code;
    let client = NodeClient::new(
        DeviceKey::open(&dir.join("phone/device.key")).unwrap(),
        pair(&address, &code, &key, "phone").unwrap(),
        "phone",
    );
    let long = "word ".repeat(200);
    let cancel = AtomicBool::new(false);
    let mut tokens = 0;
    let result = client.generate(
        &request(&long),
        &mut |_| {
            tokens += 1;
            if tokens == 3 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert!(matches!(result, Err(HybridError::Cancelled)), "{result:?}");
    assert!(tokens < 50, "the node kept going: {tokens} tokens");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn garbage_and_silent_connections_do_not_disturb_the_node() {
    let (node, address, dir) = node("garbage");
    let mut junk = TcpStream::connect(&address).unwrap();
    junk.write_all(&[0, 0, 0, 5, 1, 2, 3, 4, 5]).unwrap();
    let mut huge = TcpStream::connect(&address).unwrap();
    huge.write_all(&u32::MAX.to_be_bytes()).unwrap();
    let _silent = TcpStream::connect(&address).unwrap();

    let key = DeviceKey::open(&dir.join("phone/device.key")).unwrap();
    let code = node.start_pairing().unwrap().code;
    let client = NodeClient::new(
        DeviceKey::open(&dir.join("phone/device.key")).unwrap(),
        pair(&address, &code, &key, "phone").unwrap(),
        "phone",
    );
    assert_eq!(client.model().unwrap(), "echo");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_unreachable_node_is_reported_as_such() {
    let dir = temp("unreachable");
    let key = DeviceKey::open(&dir.join("device.key")).unwrap();
    let node = Peer::new("gone", &[7u8; 32], Some("127.0.0.1:1".to_string()));
    let client = NodeClient::new(key, node, "phone");
    assert!(matches!(client.model(), Err(HybridError::Unreachable(_))));
    let _ = std::fs::remove_dir_all(dir);
}
