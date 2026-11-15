//! The fetcher behind `read_web_page` (docs RFC-002), and the Settings switch for it.
//!
//! HTTPS only, redirects included; a GET with no cookies, credentials or referrer; and
//! never an address on the user's own network: a resolver drops loopback, private,
//! link-local and other non-public addresses for every connection, redirects included,
//! and ureq connects only to the addresses it returns, so a DNS answer cannot change
//! between the check and the connection. At most 2 MB and 20 seconds a page. The page
//! itself is read by `pr3_tool::web`.

use std::fmt;
use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pr3_tool::web::{Fetch, Fetched, MEDIA_TYPES};
use serde::Serialize;
use ureq::ResponseExt as _;
use ureq::config::Config;
use ureq::http::Uri;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

const MAX_BODY_BYTES: u64 = 2 * 1024 * 1024;
const MAX_REDIRECTS: u32 = 5;
const CONNECT: Duration = Duration::from_secs(10);
const WHOLE: Duration = Duration::from_secs(20);
/// The file in the app's data folder that turns reading web pages off, if it is there.
const OFF: &str = "web-off";
const IS_OFF: &str = "Reading web pages is off in PR3's Settings, under Web.";

/// Whether PR3 reads the pages the user links: on unless the user turned it off in
/// Settings, and unavailable when `[web] enabled` is false in the config.
pub struct Web {
    on: Arc<AtomicBool>,
    dir: Option<PathBuf>,
    available: bool,
}

/// What the Settings tab shows.
#[derive(Debug, Clone, Serialize)]
pub struct WebInfo {
    pub available: bool,
    pub on: bool,
}

/// The switch, and the fetcher for the tool when the config allows it.
pub fn open(dir: Option<&Path>, enabled: bool) -> (Web, Option<Box<dyn Fetch>>) {
    let on = !dir.is_some_and(|dir| dir.join(OFF).exists());
    let web = Web {
        on: Arc::new(AtomicBool::new(on)),
        dir: dir.map(Path::to_path_buf),
        available: enabled,
    };
    let fetcher = enabled.then(|| Box::new(Fetcher::new(Arc::clone(&web.on))) as Box<dyn Fetch>);
    (web, fetcher)
}

impl Web {
    pub fn info(&self) -> WebInfo {
        WebInfo {
            available: self.available,
            on: self.available && self.on.load(Ordering::Relaxed),
        }
    }

    /// Turns reading on or off, from now on and on later starts. The tool stays offered
    /// to the model either way, so its prompt does not change; while off, it refuses.
    pub fn set(&self, on: bool) -> Result<(), String> {
        let dir = self.dir.as_ref().ok_or("the app has no data folder")?;
        let path = dir.join(OFF);
        let result = if on {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        } else {
            fs::create_dir_all(dir).and_then(|()| fs::write(&path, "off\n"))
        };
        result.map_err(|e| e.to_string())?;
        self.on.store(on, Ordering::Relaxed);
        Ok(())
    }
}

#[tauri::command]
pub fn web_reading(web: tauri::State<'_, Web>) -> WebInfo {
    web.info()
}

#[tauri::command]
pub fn set_web_reading(on: bool, web: tauri::State<'_, Web>) -> Result<WebInfo, String> {
    web.set(on)?;
    Ok(web.info())
}

struct Fetcher {
    agent: ureq::Agent,
    on: Arc<AtomicBool>,
}

/// Optional official search API. No Brave browser dependency, no model-supplied
/// endpoint/headers, and no redirects that could forward the subscription token.
pub(crate) struct Search {
    agent: ureq::Agent,
    library: pr3_tool::documents::Library,
    fetch: Arc<dyn Fetch>,
}
impl Search {
    pub fn new(library: pr3_tool::documents::Library, fetch: Arc<dyn Fetch>) -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            .proxy(None)
            .max_redirects(0)
            .timeout_connect(Some(CONNECT))
            .timeout_global(Some(WHOLE))
            .build();
        Self {
            agent: ureq::Agent::with_parts(config, DefaultConnector::new(), PublicOnly::default()),
            library,
            fetch,
        }
    }
}
impl pr3_tool::research::SearchWeb for Search {
    fn search(&self, user_topic: &str) -> Result<Vec<pr3_tool::research::SearchHit>, String> {
        self.fetch.check_enabled()?;
        let key=zeroize::Zeroizing::new(self.library.lock().map_err(|_|"Research storage is unavailable")?
            .research_search_key().map_err(|e|e.to_string())?.ok_or("Add a Brave Search API key in Settings → Web for automatic search. Provided links still work.")?);
        let mut response = self
            .agent
            .get("https://api.search.brave.com/res/v1/web/search")
            .query("q", user_topic)
            .query("count", "5")
            .query("text_decorations", "false")
            .header("Accept", "application/json")
            .header("X-Subscription-Token", key.as_str())
            .call()
            .map_err(|_| "Search could not connect securely within its time limit".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Search provider returned HTTP {}. Check the API key, quota and connection.",
                response.status().as_u16()
            ));
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(512 * 1024)
            .read_to_vec()
            .map_err(|_| "Search response could not be read within its size/time limit")?;
        self.fetch.check_enabled()?;
        parse_search(&bytes)
    }
}
fn parse_search(bytes: &[u8]) -> Result<Vec<pr3_tool::research::SearchHit>, String> {
    let body: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "Search provider returned invalid JSON")?;
    let rows = body
        .pointer("/web/results")
        .and_then(serde_json::Value::as_array)
        .ok_or("Search provider returned no valid web-results list")?;
    rows.iter()
        .take(5)
        .map(|row| {
            Ok(pr3_tool::research::SearchHit {
                url: row
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("Search result has no URL")?
                    .into(),
                title: row
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("Search result has no title")?
                    .into(),
                snippet: row
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .into(),
            })
        })
        .collect()
}

impl Fetcher {
    fn new(on: Arc<AtomicBool>) -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            // A proxy would resolve the site itself, past the resolver's check.
            .proxy(None)
            .max_redirects(MAX_REDIRECTS)
            .timeout_connect(Some(CONNECT))
            .timeout_global(Some(WHOLE))
            .user_agent(concat!("PR3/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            agent: ureq::Agent::with_parts(config, DefaultConnector::new(), PublicOnly::default()),
            on,
        }
    }
}

impl Fetch for Fetcher {
    fn check_enabled(&self) -> Result<(), String> {
        if !self.on.load(Ordering::Relaxed) {
            return Err(IS_OFF.to_string());
        }
        Ok(())
    }

    fn fetch(&self, url: &str) -> Result<Fetched, String> {
        self.check_enabled()?;
        let mut response = self
            .agent
            .get(url)
            .header(
                "Accept",
                "text/html, application/xhtml+xml, text/plain;q=0.9",
            )
            .call()
            .map_err(reason)?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("the site answered {status}"));
        }
        let url = response.get_uri().to_string();
        let media_type = response
            .body()
            .mime_type()
            .unwrap_or_default()
            .to_ascii_lowercase();
        // A type PR3 does not read is refused by the tool, which names it; its body is
        // never downloaded.
        let body = if MEDIA_TYPES.contains(&media_type.as_str()) {
            response
                .body_mut()
                .with_config()
                .limit(MAX_BODY_BYTES)
                .read_to_vec()
                .map_err(reason)?
        } else {
            Vec::new()
        };
        Ok(Fetched {
            url,
            media_type,
            body,
        })
    }
}

/// Why a fetch failed, in words the model can pass on.
fn reason(error: ureq::Error) -> String {
    match error {
        ureq::Error::RequireHttpsOnly(_) => {
            "the site does not offer HTTPS, which PR3 requires".to_string()
        }
        ureq::Error::Io(e) if e.get_ref().is_some_and(|inner| inner.is::<LocalAddress>()) => {
            LocalAddress.to_string()
        }
        ureq::Error::HostNotFound => "no site has that name".to_string(),
        ureq::Error::Timeout(_) => {
            format!("the site did not answer within {} seconds", WHOLE.as_secs())
        }
        ureq::Error::TooManyRedirects => {
            format!("the site redirected more than {MAX_REDIRECTS} times")
        }
        ureq::Error::BodyExceedsLimit(_) => format!(
            "the page is larger than {} MB, more than PR3 reads",
            MAX_BODY_BYTES / (1024 * 1024)
        ),
        ureq::Error::ConnectionFailed => "PR3 could not connect to the site".to_string(),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => {
            "the site's secure connection could not be verified".to_string()
        }
        other => format!("the page could not be read: {other}"),
    }
}

/// The resolver's refusal: every address the name has is on a private network.
#[derive(Debug)]
struct LocalAddress;

impl fmt::Display for LocalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PR3 does not read addresses on your own network")
    }
}

impl std::error::Error for LocalAddress {}

/// The default resolver, keeping only public addresses.
#[derive(Debug, Default)]
struct PublicOnly(DefaultResolver);

impl Resolver for PublicOnly {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let all = self.0.resolve(uri, config, timeout)?;
        let mut public = self.empty();
        for addr in all.iter().filter(|addr| public_ip(addr.ip())) {
            public.push(*addr);
        }
        if public.is_empty() {
            return Err(ureq::Error::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                LocalAddress,
            )));
        }
        Ok(public)
    }
}

/// Whether `ip` is on the public internet: not loopback, private, shared, link-local
/// (with the cloud metadata address), unique-local, multicast, reserved or for
/// documentation, in IPv4 or in the IPv6 forms that carry an IPv4 address.
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..20).contains(&b))
        || a >= 240)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let s = ip.segments();
    let embedded = |hi: u16, lo: u16| {
        let [a, b] = hi.to_be_bytes();
        let [c, d] = lo.to_be_bytes();
        Ipv4Addr::new(a, b, c, d)
    };
    // NAT64 (64:ff9b::/96), as on mobile networks without IPv4: the IPv4 address decides.
    if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        return public_v4(embedded(s[6], s[7]));
    }
    // 6to4 (2002::/16) carries one too.
    if s[0] == 0x2002 {
        return public_v4(embedded(s[1], s[2]));
    }
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00
        || (s[0] & 0xffc0) == 0xfe80
        || (s[0] == 0x2001 && s[1] == 0x0db8)
        || s[..6] == [0, 0, 0, 0, 0, 0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_responses_distinguish_empty_results_from_provider_errors() {
        assert!(
            parse_search(br#"{"web":{"results":[]}}"#)
                .unwrap()
                .is_empty()
        );
        assert!(parse_search(br#"{"error":"quota exhausted"}"#).is_err());
        assert!(parse_search(b"<html>challenge</html>").is_err());
        assert!(parse_search(br#"{"web":{"results":[{"invalid":true}]}}"#).is_err());
        let hits=parse_search(br#"{"web":{"results":[{"title":"Plan","url":"https://example.org/","description":"Cost 40"}]}}"#).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.org/");
        assert_eq!(hits[0].snippet, "Cost 40");
    }

    #[test]
    fn search_without_a_key_or_with_web_off_fails_before_network() {
        use pr3_tool::research::SearchWeb;
        let db = Arc::new(std::sync::Mutex::new(
            pr3_memory_sqlite::SqliteMemory::open_in_memory().unwrap(),
        ));
        let on = Arc::new(AtomicBool::new(true));
        let search = Search::new(db, Arc::new(Fetcher::new(on.clone())));
        assert!(
            search
                .search("public topic")
                .err()
                .unwrap()
                .contains("API key")
        );
        on.store(false, Ordering::Relaxed);
        assert_eq!(search.search("public topic").err().unwrap(), IS_OFF);
    }

    fn fetcher() -> Fetcher {
        Fetcher::new(Arc::new(AtomicBool::new(true)))
    }

    #[test]
    fn only_public_addresses_are_reached() {
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
            "::ffff:192.168.1.1",
            "64:ff9b::a00:1",
            "2002:c0a8:101::1",
            "::127.0.0.1",
        ] {
            let ip: IpAddr = private.parse().unwrap();
            assert!(!public_ip(ip), "{private}");
        }
        for public in [
            "93.184.215.14",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "64:ff9b::5db8:d70e",
            "::ffff:93.184.215.14",
        ] {
            let ip: IpAddr = public.parse().unwrap();
            assert!(public_ip(ip), "{public}");
        }
    }

    #[test]
    fn local_addresses_are_refused_before_connecting() {
        for url in [
            "https://127.0.0.1:9/",
            "https://localhost:9/",
            "https://[::1]:9/",
            "https://10.0.0.1:9/",
            "https://169.254.169.254/latest/meta-data/",
        ] {
            assert_eq!(
                fetcher().fetch(url),
                Err("PR3 does not read addresses on your own network".to_string()),
                "{url}"
            );
        }
    }

    #[test]
    fn plain_http_is_refused() {
        assert_eq!(
            fetcher().fetch("http://example.com/"),
            Err("the site does not offer HTTPS, which PR3 requires".to_string())
        );
    }

    #[test]
    fn the_switch_stops_fetching_and_is_kept() {
        let dir = std::env::temp_dir().join(format!("pr3-web-{}", std::process::id()));
        let (web, fetcher) = open(Some(&dir), true);
        let fetcher = fetcher.unwrap();
        assert!(web.info().on);
        assert!(fetcher.check_enabled().is_ok());
        web.set(false).unwrap();
        assert_eq!(fetcher.check_enabled(), Err(IS_OFF.to_string()));
        assert_eq!(
            fetcher.fetch("https://example.com/"),
            Err(IS_OFF.to_string())
        );
        let (again, _) = open(Some(&dir), true);
        assert!(!again.info().on, "off stays off on the next start");
        again.set(true).unwrap();
        assert!(open(Some(&dir), true).0.info().on);
        web.set(true).unwrap();
        assert!(fetcher.check_enabled().is_ok());
        let (disabled, none) = open(Some(&dir), false);
        assert!(none.is_none() && !disabled.info().available && !disabled.info().on);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The real network, through the tool: a page read whole, and a link that redirects.
    /// Opt-in, as it needs the internet:
    /// cargo test -p pr3-app -- --ignored real_pages
    #[test]
    #[ignore = "needs the internet"]
    fn real_pages_are_read() {
        use pr3_tool::{Tool, ToolContext, web::ReadWebPage};
        let tool = ReadWebPage::new(Box::new(fetcher()));
        let read = |said: &str, url: &str| {
            let ctx = ToolContext {
                now: pr3_core::Timestamp(0),
                utc_offset_minutes: 0,
                session: pr3_core::SessionId(1),
                utterance: said.to_string(),
                earlier: Vec::new(),
            };
            let args = serde_json::from_str(&format!(r#"{{"url":"{url}"}}"#)).unwrap();
            tool.run(&args, &ctx)
        };
        let page = read("what is example.com?", "example.com").unwrap();
        eprintln!("{page}");
        assert!(page.contains("Example Domain"), "{page}");
        let moved = read("open http://rust-lang.org", "http://rust-lang.org").unwrap();
        eprintln!("{}", moved.lines().next().unwrap_or_default());
        assert!(moved.starts_with("Web page: https://"), "{moved}");
    }
}
