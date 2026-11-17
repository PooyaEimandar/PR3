//! Website scope comes only from the persisted, user-submitted research topic.
//! Titles, snippets, model arguments and page instructions cannot change it.
use pr3_memory_sqlite::research::Evidence;
use url::Url;

#[derive(Clone, Debug, Default)]
pub struct SourceScope {
    hosts: Vec<String>,
    problem: Option<String>,
}
impl SourceScope {
    /// Persisted user corrections can narrow the original scope. Neither ordinary
    /// continuation nor a broader list can restore excluded websites.
    pub fn from_user<'a>(topic: &str, messages: impl IntoIterator<Item = &'a str>) -> Self {
        let mut scope = Self::from_topic(topic);
        for message in messages {
            let next = Self::from_topic(message);
            if !next.restricted() {
                continue;
            }
            if next.problem.is_some() {
                scope.problem = next.problem;
                continue;
            }
            if scope.hosts.is_empty() {
                scope = next;
                continue;
            }
            let mut hosts = Vec::new();
            for a in &scope.hosts {
                for b in &next.hosts {
                    let narrower = if host_within(a, b) {
                        Some(a)
                    } else if host_within(b, a) {
                        Some(b)
                    } else {
                        None
                    };
                    if let Some(host) = narrower
                        && !hosts.contains(host)
                    {
                        hosts.push(host.clone());
                    }
                }
            }
            if hosts.is_empty() {
                scope.problem = Some("The requested websites do not overlap this task's website scope. Start a new research task to change it.".into());
            } else {
                scope.hosts = hosts;
                scope.problem = None;
            }
        }
        scope
    }
    /// The UI appends this explicit clause. It is also available in typed research
    /// requests. Scope is immutable for a task, like its original web search topic.
    pub fn from_topic(topic: &str) -> Self {
        let lower = topic.to_ascii_lowercase();
        if let Some(at) = lower.rfind("sources only:") {
            let tail = &topic[at + "sources only:".len()..];
            match parse_hosts(tail) {
                Ok(hosts) => Self {
                    hosts,
                    problem: None,
                },
                Err(problem) => Self {
                    hosts: vec![],
                    problem: Some(problem),
                },
            }
        } else if lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w == "official")
            && lower
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| matches!(w, "only" | "exclusively"))
        {
            Self { hosts: vec![], problem: Some("To limit research to official sources, choose their websites in the research dialog, or start a new task ending with Sources only: example.org. PR3 cannot verify official status from a search result's title or snippet.".into()) }
        } else {
            Self::default()
        }
    }
    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }
    pub fn restricted(&self) -> bool {
        !self.hosts.is_empty() || self.problem.is_some()
    }
    pub fn allows_url(&self, url: &str) -> bool {
        self.problem.is_none() && (self.hosts.is_empty() || url_in_scope(url, &self.hosts))
    }
    pub fn allows_evidence(&self, source: &Evidence) -> bool {
        if !self.restricted() {
            return true;
        }
        source.document.is_none()
            && self.allows_url(source.location.split(" · part ").next().unwrap_or_default())
    }
}
fn host_within(host: &str, allowed: &str) -> bool {
    host == allowed || host.strip_suffix(allowed).is_some_and(|s| s.ends_with('.'))
}

fn parse_hosts(text: &str) -> Result<Vec<String>, String> {
    let mut hosts = Vec::new();
    for value in text.split(',').map(str::trim) {
        // Domains, not paths, query strings, credentials, IP addresses or wildcards.
        if value.is_empty()
            || value.len() > 253
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return Err("Sources only: needs 1–4 comma-separated website domains, such as sqlite.org, docs.example.org.".into());
        }
        let host = value.to_ascii_lowercase();
        if !host.contains('.')
            || host.ends_with('.')
            || host
                .split('.')
                .any(|p| p.is_empty() || p.len() > 63 || p.starts_with('-') || p.ends_with('-'))
            || host.parse::<std::net::IpAddr>().is_ok()
        {
            return Err("Choose website domains, not IP addresses or local hostnames.".into());
        }
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    if hosts.len() > 4 {
        return Err("Choose at most four source websites.".into());
    }
    Ok(hosts)
}

/// Checked before each network hop by the production fetcher, and again before
/// saving/citing the result. Normal public-network/DNS checks remain separate.
pub fn url_in_scope(value: &str, hosts: &[String]) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    hosts.iter().any(|allowed| host_within(host, allowed))
}

pub fn scoped_redirect(current: &str, location: &str, hosts: &[String]) -> Result<String, String> {
    let next = Url::parse(current)
        .and_then(|u| u.join(location))
        .map_err(|_| "Invalid page redirect")?;
    if !url_in_scope(next.as_str(), hosts) {
        return Err("Page redirected outside this task's website restriction".into());
    }
    Ok(next.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn website_scope_rejects_lookalikes_credentials_and_invalid_lists() {
        let scope =
            SourceScope::from_topic("Compare plans. Sources only: EXAMPLE.org, docs.example.net");
        for url in [
            "https://example.org/a",
            "https://sub.example.org/a",
            "https://docs.example.net/",
        ] {
            assert!(scope.allows_url(url));
        }
        for url in [
            "https://example.org.attacker.net/",
            "https://notexample.org/",
            "https://example.org@attacker.net/",
            "https://a@example.org/",
            "http://example.org/",
            "https://example.org:444/",
            "https://example.net/",
            "https://example.org./",
        ] {
            assert!(!scope.allows_url(url), "{url}");
        }
        for value in [
            "",
            "*.example.org",
            "https://example.org",
            "127.0.0.1",
            "localhost",
            "example.org/path",
            "example..org",
            "-example.org",
            "a.org,b.org,c.org,d.org,e.org",
        ] {
            let scope = SourceScope::from_topic(&format!("Sources only: {value}"));
            assert!(scope.problem().is_some(), "{value}");
            assert!(!scope.allows_url("https://example.org/"));
        }
    }
    #[test]
    fn official_request_needs_user_selected_websites() {
        let scope = SourceScope::from_topic("Compare plans using only official sources");
        assert!(scope.problem().unwrap().contains("choose their websites"));
        assert!(!scope.allows_url("https://official.example.org/"));
        assert!(!SourceScope::from_topic("Compare plans").restricted());
        assert!(!SourceScope::from_topic("Compare only unofficial plans").restricted());
    }
    #[test]
    fn saved_corrections_narrow_scope_and_cannot_widen_it_after_restart() {
        let scope = SourceScope::from_user(
            "Compare plans. Sources only: example.org, example.net",
            [
                "Sources only: docs.example.org",
                "Continue",
                "Sources only: example.org, example.net",
            ],
        );
        assert_eq!(scope.hosts(), &["docs.example.org"]);
        assert!(!scope.allows_url("https://example.net/"));
        let disjoint = SourceScope::from_user(
            "Sources only: example.org",
            ["Sources only: example.net", "Continue"],
        );
        assert!(disjoint.problem().is_some());
        assert!(!disjoint.allows_url("https://example.org/"));
        let clarified =
            SourceScope::from_user("Use official sources only", ["Sources only: example.org"]);
        assert!(clarified.allows_url("https://example.org/"));
    }
    #[test]
    fn redirect_scope_is_checked_before_following_relative_or_absolute_links() {
        let hosts = vec!["example.org".into()];
        assert_eq!(
            scoped_redirect("https://example.org/docs/a", "../b", &hosts).unwrap(),
            "https://example.org/b"
        );
        for location in [
            "//attacker.org/a",
            "https://example.org.attacker.org/",
            "http://example.org/",
            "https://example.org:444/",
            "https://user:pass@example.org/",
        ] {
            assert!(scoped_redirect("https://example.org/a", location, &hosts).is_err());
        }
    }
}
