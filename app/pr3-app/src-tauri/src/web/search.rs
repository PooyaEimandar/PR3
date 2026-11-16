//! Fixed, optional search providers. Search snippets are candidates, never evidence.
use super::{CONNECT, PublicOnly, WHOLE};
use pr3_memory_sqlite::research::SearchProvider;
use pr3_tool::{documents::Library, research::SearchHit, web::Fetch};
use std::sync::Arc;
use ureq::unversioned::transport::DefaultConnector;

pub(crate) struct Search {
    agent: ureq::Agent,
    library: Library,
    fetch: Arc<dyn Fetch>,
}
fn config() -> ureq::config::Config {
    ureq::Agent::config_builder()
        .https_only(true)
        .http_status_as_error(false)
        .proxy(None)
        .max_redirects(0)
        .timeout_connect(Some(CONNECT))
        .timeout_global(Some(WHOLE))
        .build()
}
impl Search {
    pub fn new(library: Library, fetch: Arc<dyn Fetch>) -> Self {
        Self {
            agent: ureq::Agent::with_parts(
                config(),
                DefaultConnector::new(),
                PublicOnly::default(),
            ),
            library,
            fetch,
        }
    }
}
fn secret_header(value: &str) -> Result<ureq::http::HeaderValue, String> {
    let mut header = ureq::http::HeaderValue::from_str(value)
        .map_err(|_| "Invalid search API key. Replace it in Settings → Web.")?;
    header.set_sensitive(true);
    Ok(header)
}
impl pr3_tool::research::SearchWeb for Search {
    fn search(&self, user_topic: &str) -> Result<Vec<SearchHit>, String> {
        self.fetch.check_enabled()?;
        // Read selection and its credential under one lock. Never fall back to a
        // different provider or retry a billable request automatically.
        let (provider, key) = {
            let db = self
                .library
                .lock()
                .map_err(|_| "Research storage is unavailable")?;
            let provider = db.research_search_provider().map_err(|e| e.to_string())?;
            let key = zeroize::Zeroizing::new(db.research_search_key(provider)
                .map_err(|e| e.to_string())?.ok_or_else(|| format!(
                    "Add a {} API key in Settings → Web for automatic search. Provided links still work.", provider.name()))?);
            (provider, key)
        };
        let response = match provider {
            SearchProvider::Tavily => {
                let bearer = zeroize::Zeroizing::new(format!("Bearer {}", key.as_str()));
                let body = serde_json::json!({
                    "query": user_topic,
                    "search_depth": "basic",
                    "auto_parameters": false,
                    "topic": "general",
                    "max_results": 5,
                    "include_answer": false,
                    "include_raw_content": false,
                    "include_images": false,
                })
                .to_string();
                self.agent
                    .post("https://api.tavily.com/search")
                    .header("Authorization", secret_header(bearer.as_str())?)
                    .header("Accept", "application/json")
                    .header("Content-Type", "application/json")
                    .send(body.as_bytes())
            }
            SearchProvider::Brave => self
                .agent
                .get("https://api.search.brave.com/res/v1/web/search")
                .query("q", user_topic)
                .query("count", "5")
                .query("text_decorations", "false")
                .header("Accept", "application/json")
                .header("X-Subscription-Token", secret_header(key.as_str())?)
                .call(),
        };
        let mut response = response
            .map_err(|_| "Search could not connect securely within its time limit".to_string())?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let detail = match status {
                401 | 403 => "Check the API key in Settings → Web.",
                429 | 432 | 433 => {
                    "Search quota or rate limit reached. Check your provider account or try again later."
                }
                _ => "Check the API key, quota and connection.",
            };
            // Provider bodies may echo secrets or the query: do not expose them.
            return Err(format!(
                "{} returned HTTP {status}. {detail}",
                provider.name()
            ));
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(512 * 1024)
            .read_to_vec()
            .map_err(|_| "Search response could not be read within its size/time limit")?;
        self.fetch.check_enabled()?;
        parse_search(provider, &bytes)
    }
}
fn parse_search(provider: SearchProvider, bytes: &[u8]) -> Result<Vec<SearchHit>, String> {
    let body: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "Search provider returned invalid JSON")?;
    let (path, snippet) = match provider {
        SearchProvider::Tavily => ("/results", "content"),
        SearchProvider::Brave => ("/web/results", "description"),
    };
    let rows = body
        .pointer(path)
        .and_then(serde_json::Value::as_array)
        .ok_or("Search provider returned no valid web-results list")?;
    rows.iter()
        .take(5)
        .map(|row| {
            Ok(SearchHit {
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
                    .get(snippet)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
