use super::*;
use pr3_memory_sqlite::{
    SqliteMemory,
    research::SearchProvider::{Brave, Tavily},
};
use pr3_tool::research::SearchWeb;
use std::{
    io::Read,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use ureq::{
    config::Config,
    http::Uri,
    unversioned::{
        resolver::{ResolvedSocketAddrs, Resolver},
        transport::{Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, Transport},
    },
};

// Exercise the real HTTP serializer/parser and production config without sockets,
// live credentials or paid requests. Only DNS and the byte transport are fixtures.
#[derive(Debug, Default)]
struct Recorded {
    uris: Vec<String>,
    wire: Vec<u8>,
}
#[derive(Debug)]
struct Fixture {
    response: Vec<u8>,
    recorded: Arc<Mutex<Recorded>>,
    disable: Option<Arc<AtomicBool>>,
}
#[derive(Debug)]
struct Connection {
    fixture: Fixture,
    buffers: LazyBuffers,
    response: std::io::Cursor<Vec<u8>>,
}
impl Connector for Fixture {
    type Out = Connection;
    fn connect(
        &self,
        details: &ConnectionDetails,
        _: Option<()>,
    ) -> Result<Option<Connection>, ureq::Error> {
        self.recorded
            .lock()
            .unwrap()
            .uris
            .push(details.uri.to_string());
        Ok(Some(Connection {
            fixture: Fixture {
                response: vec![],
                recorded: self.recorded.clone(),
                disable: self.disable.clone(),
            },
            buffers: LazyBuffers::new(4096, 4096),
            response: std::io::Cursor::new(self.response.clone()),
        }))
    }
}
impl Transport for Connection {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }
    fn transmit_output(&mut self, amount: usize, _: NextTimeout) -> Result<(), ureq::Error> {
        self.fixture
            .recorded
            .lock()
            .unwrap()
            .wire
            .extend_from_slice(&self.buffers.output()[..amount]);
        Ok(())
    }
    fn await_input(&mut self, _: NextTimeout) -> Result<bool, ureq::Error> {
        if let Some(on) = &self.fixture.disable {
            on.store(false, Ordering::Relaxed);
        }
        let count = self.response.read(self.buffers.input_append_buf())?;
        self.buffers.input_appended(count);
        Ok(count > 0)
    }
    fn is_open(&mut self) -> bool {
        false
    }
    fn is_tls(&self) -> bool {
        true
    }
}
#[derive(Debug)]
struct FixedDns;
impl Resolver for FixedDns {
    fn resolve(
        &self,
        _: &Uri,
        _: &Config,
        _: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let mut addresses = self.empty();
        addresses.push("93.184.215.14:443".parse().unwrap());
        Ok(addresses)
    }
}
fn fixture(
    provider: SearchProvider,
    status: u16,
    body: &str,
    disable: bool,
) -> (Search, Arc<Mutex<Recorded>>, Arc<AtomicBool>) {
    let mut db = SqliteMemory::open_in_memory().unwrap();
    db.set_research_search_provider(provider).unwrap();
    db.set_research_search_key(Tavily, "synthetic-tavily-key")
        .unwrap();
    db.set_research_search_key(Brave, "synthetic-brave-key")
        .unwrap();
    let on = Arc::new(AtomicBool::new(true));
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let connector = Fixture {
        response: format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nLocation: https://attacker.example/\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes(),
        recorded: recorded.clone(), disable: disable.then(|| on.clone()),
    };
    (
        Search {
            agent: ureq::Agent::with_parts(config(), connector, FixedDns),
            library: Arc::new(Mutex::new(db)),
            fetch: Arc::new(super::super::Fetcher::new(on.clone())),
        },
        recorded,
        on,
    )
}

#[test]
fn tavily_sends_only_the_topic_using_basic_search_and_ignores_generated_answers() {
    let (search, recorded, _) = fixture(
        Tavily,
        200,
        r#"{"results":[{"title":"Plan","url":"https://example.org/","content":"Cost 40","raw_content":"PRIVATE RAW TEXT"}],"answer":"UNVERIFIED ANSWER"}"#,
        false,
    );
    let topic = "Compare café prices & هزینه \"today\"";
    let hits = search.search(topic).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "Plan");
    assert_eq!(hits[0].url, "https://example.org/");
    assert_eq!(hits[0].snippet, "Cost 40");
    let recorded = recorded.lock().unwrap();
    assert_eq!(recorded.uris, ["https://api.tavily.com/search"]);
    let wire = String::from_utf8(recorded.wire.clone()).unwrap();
    let (headers, body) = wire.split_once("\r\n\r\n").unwrap();
    let headers = headers.to_lowercase();
    assert!(headers.starts_with("post /search http/1.1\r\n"));
    assert!(headers.contains("authorization: bearer synthetic-tavily-key\r\n"));
    assert!(
        headers
            .lines()
            .any(|line| line == "content-type: application/json")
    );
    assert!(!wire.contains("synthetic-brave-key"));
    let body: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"query":topic,"search_depth":"basic","auto_parameters":false,"topic":"general","max_results":5,"include_answer":false,"include_raw_content":false,"include_images":false})
    );
    assert!(secret_header("synthetic-key").unwrap().is_sensitive());
}

#[test]
fn brave_remains_explicit_and_uses_its_own_key_and_encoded_topic() {
    let (search, recorded, _) = fixture(
        Brave,
        200,
        r#"{"web":{"results":[{"title":"Plan","url":"https://example.org/","description":"Cost 25"}]}}"#,
        false,
    );
    assert_eq!(
        search.search("price & count=100").unwrap()[0].snippet,
        "Cost 25"
    );
    let recorded = recorded.lock().unwrap();
    assert_eq!(recorded.uris.len(), 1);
    assert_eq!(
        recorded.uris[0],
        "https://api.search.brave.com/res/v1/web/search?q=price%20%26%20count%3D100&count=5&text_decorations=false"
    );
    let wire = String::from_utf8(recorded.wire.clone())
        .unwrap()
        .to_lowercase();
    assert!(wire.starts_with("get /res/v1/web/search?"));
    assert!(wire.contains("x-subscription-token: synthetic-brave-key\r\n"));
    assert!(!wire.contains("synthetic-tavily-key"));
}

#[test]
fn provider_errors_and_redirects_do_not_retry_fall_back_or_echo_response_bodies() {
    for provider in [Tavily, Brave] {
        for status in [301, 307, 400, 401, 403, 429, 432, 433, 500] {
            let (search, recorded, _) = fixture(
                provider,
                status,
                "secret-echo synthetic-tavily-key synthetic-brave-key",
                false,
            );
            let error = search.search("public topic").err().unwrap();
            assert!(!error.contains("secret-echo") && !error.contains("synthetic-"));
            assert_eq!(
                recorded.lock().unwrap().uris.len(),
                1,
                "no retry or redirected credentials"
            );
            if [429, 432, 433].contains(&status) {
                assert!(error.contains("quota or rate limit"));
            }
        }
    }
}

#[test]
fn selected_key_and_web_permission_are_required_and_responses_remain_bounded() {
    for provider in [Tavily, Brave] {
        let (search, recorded, on) = fixture(
            provider,
            200,
            r#"{"results":[],"web":{"results":[]}}"#,
            false,
        );
        search
            .library
            .lock()
            .unwrap()
            .set_research_search_key(provider, "")
            .unwrap();
        assert!(
            search
                .search("topic")
                .err()
                .unwrap()
                .contains(provider.name())
        );
        assert!(
            recorded.lock().unwrap().uris.is_empty(),
            "other saved key cannot authorize fallback"
        );
        search
            .library
            .lock()
            .unwrap()
            .set_research_search_key(provider, "synthetic-replacement-key")
            .unwrap();
        on.store(false, Ordering::Relaxed);
        assert_eq!(search.search("topic").err().unwrap(), super::super::IS_OFF);
        assert!(recorded.lock().unwrap().uris.is_empty());
        let (search, _, _) = fixture(
            provider,
            200,
            r#"{"results":[],"web":{"results":[]}}"#,
            true,
        );
        assert_eq!(
            search.search("topic").err().unwrap(),
            super::super::IS_OFF,
            "disable while in flight discards results"
        );
        let (search, _, _) = fixture(provider, 200, &" ".repeat(512 * 1024 + 1), false);
        assert!(
            search
                .search("topic")
                .err()
                .unwrap()
                .contains("size/time limit")
        );
    }
    assert!(config().https_only());
    assert!(config().proxy().is_none());
    assert_eq!(config().max_redirects(), 0);
    assert_eq!(config().timeouts().global, Some(WHOLE));
    assert_eq!(config().timeouts().connect, Some(CONNECT));
}

#[test]
fn empty_malformed_and_excess_results_are_handled_for_each_provider() {
    for provider in [Tavily, Brave] {
        assert!(
            parse_search(provider, br#"{"results":[],"web":{"results":[]}}"#)
                .unwrap()
                .is_empty()
        );
        for bad in [
            br#"{"error":"quota exhausted"}"#.as_slice(),
            b"<html>challenge</html>",
            br#"{"results":[{}],"web":{"results":[{}]}}"#,
        ] {
            assert!(parse_search(provider, bad).is_err());
        }
        let rows = vec![serde_json::json!({"title":"Plan","url":"https://example.org/"}); 8];
        let body = serde_json::to_vec(&serde_json::json!({"results":rows,"web":{"results":rows}}))
            .unwrap();
        let hits = parse_search(provider, &body).unwrap();
        assert_eq!(hits.len(), 5);
        assert!(hits.iter().all(|hit| hit.snippet.is_empty()));
    }
}
