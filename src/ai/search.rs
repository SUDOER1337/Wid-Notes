use serde::{Deserialize, Serialize};
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug)]
pub enum SearchError {
    Misconfigured(String),
    RequestFailed(String),
    ParseFailed(String),
    NoResults,
}

fn search_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
        .expect("Failed to build reqwest client")
}

// ── Shared JSON result parsing ──────────────────────────

fn parse_json_results(results: &[serde_json::Value], snippet_field: &str) -> Vec<SearchResult> {
    results
        .iter()
        .filter_map(|r| {
            let title = r["title"].as_str()?.to_string();
            let url = r["url"].as_str()?.to_string();
            let snippet = r[snippet_field].as_str().unwrap_or("").to_string();
            Some(SearchResult {
                title,
                url,
                snippet,
            })
        })
        .collect()
}

fn json_results_nonempty(
    results: &[serde_json::Value],
    snippet_field: &str,
) -> Result<Vec<SearchResult>, SearchError> {
    let items = parse_json_results(results, snippet_field);
    if items.is_empty() {
        Err(SearchError::NoResults)
    } else {
        Ok(items)
    }
}

// ── Brave Search ─────────────────────────────────────────

const BRAVE_API_URL: &str = "https://api.search.brave.com/res/v1/web/search";

#[derive(Clone)]
pub struct BraveSearch {
    api_key: String,
    client: reqwest::Client,
}

impl BraveSearch {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: search_client(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        if self.api_key.is_empty() {
            return Err(SearchError::Misconfigured(
                "Brave Search API key not set. Add it in Settings.".into(),
            ));
        }

        let resp = self
            .client
            .get(BRAVE_API_URL)
            .header("X-Subscription-Token", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("q", query), ("count", "5")])
            .send()
            .await
            .map_err(|e| SearchError::RequestFailed(e.to_string()))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SearchError::ParseFailed(e.to_string()))?;

        let results = body["web"]["results"]
            .as_array()
            .ok_or(SearchError::NoResults)?;

        json_results_nonempty(results, "description")
    }
}

// ── SearXNG (self-hosted) ────────────────────────────────

#[derive(Clone)]
pub struct SearXNG {
    base_url: String,
    client: reqwest::Client,
}

impl SearXNG {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: search_client(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty()
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        if self.base_url.is_empty() {
            return Err(SearchError::Misconfigured(
                "SearXNG URL not set. Add it in Settings.".into(),
            ));
        }

        let url = format!("{}/search", self.base_url);

        let resp = match self
            .client
            .get(&url)
            .query(&[("q", query), ("format", "json")])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(SearchError::RequestFailed(e.to_string()));
            }
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => {
                return Err(SearchError::ParseFailed(e.to_string()));
            }
        };

        let results = match body["results"].as_array() {
            Some(a) => a,
            None => {
                return Err(SearchError::NoResults);
            }
        };

        json_results_nonempty(results, "content")
    }
}

// ── DuckDuckGo (free, no API key) ────────────────────────

#[derive(Clone)]
pub struct DuckDuckGo {
    client: reqwest::Client,
}

impl DuckDuckGo {
    pub fn new() -> Self {
        Self {
            client: search_client(),
        }
    }

    pub fn is_configured(&self) -> bool {
        true
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let resp = self
            .client
            .get("https://html.duckduckgo.com/html/")
            .query(&[("q", query)])
            .send()
            .await
            .map_err(|e| SearchError::RequestFailed(e.to_string()))?;

        let html = resp
            .text()
            .await
            .map_err(|e| SearchError::ParseFailed(e.to_string()))?;

        parse_ddg_html(&html)
    }
}

fn parse_ddg_html(html: &str) -> Result<Vec<SearchResult>, SearchError> {
    let mut results = Vec::new();
    let mut pos = 0;

    while let Some(p) = html[pos..].find("class=\"result__body\"") {
        let body_start = pos + p;

        let title_link = match html[body_start..].find("class=\"result__a\"") {
            Some(p) => body_start + p,
            None => {
                pos = body_start + 1;
                continue;
            }
        };

        let href_start = match html[title_link..].find("href=\"") {
            Some(p) => title_link + p + 6,
            None => {
                pos = body_start + 1;
                continue;
            }
        };
        let href_end = match html[href_start..].find('\"') {
            Some(p) => href_start + p,
            None => {
                pos = body_start + 1;
                continue;
            }
        };
        let raw_url = &html[href_start..href_end];

        let title_start = match html[title_link..].find('>') {
            Some(p) => title_link + p + 1,
            None => {
                pos = body_start + 1;
                continue;
            }
        };
        let title_end = match html[title_start..].find("</a>") {
            Some(p) => title_start + p,
            None => {
                pos = body_start + 1;
                continue;
            }
        };

        let snippet_start = match html[body_start..].find("class=\"result__snippet\"") {
            Some(p) => body_start + p,
            None => {
                pos = body_start + 1;
                continue;
            }
        };
        let snippet_content_start = match html[snippet_start..].find('>') {
            Some(p) => snippet_start + p + 1,
            None => {
                pos = body_start + 1;
                continue;
            }
        };
        let snippet_end = match html[snippet_content_start..].find("</a>") {
            Some(p) => snippet_content_start + p,
            None => {
                pos = body_start + 1;
                continue;
            }
        };

        results.push(SearchResult {
            title: unescape_html(&html[title_start..title_end]),
            url: decode_ddg_url(raw_url),
            snippet: unescape_html(&html[snippet_content_start..snippet_end]),
        });

        pos = body_start + 1;
    }

    if results.is_empty() {
        return Err(SearchError::NoResults);
    }
    Ok(results)
}

fn unescape_html(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
}

fn decode_ddg_url(url: &str) -> String {
    let decoded = url
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">");

    if let Some(start) = decoded.find("uddg=") {
        let encoded = &decoded[start + 5..];
        let end = encoded.find('&').unwrap_or(encoded.len());
        let encoded = &encoded[..end];
        let mut result = String::with_capacity(encoded.len());
        let mut chars = encoded.chars();
        while let Some(c) = chars.next() {
            if c == '%' {
                let hex: String = chars.by_ref().take(2).collect();
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                } else {
                    result.push('%');
                    result.push_str(&hex);
                }
            } else if c == '+' {
                result.push(' ');
            } else {
                result.push(c);
            }
        }
        result
    } else {
        decoded.to_string()
    }
}

// ── Tavily (AI-optimized) ────────────────────────────────

const TAVILY_API_URL: &str = "https://api.tavily.com/search";

#[derive(Clone)]
pub struct Tavily {
    api_key: String,
    client: reqwest::Client,
}

impl Tavily {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: search_client(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        if self.api_key.is_empty() {
            return Err(SearchError::Misconfigured(
                "Tavily API key not set. Add it in Settings.".into(),
            ));
        }

        let body = serde_json::json!({
            "api_key": self.api_key,
            "query": query,
            "search_depth": "basic",
            "max_results": 5,
        });

        let resp = self
            .client
            .post(TAVILY_API_URL)
            .json(&body)
            .send()
            .await
            .map_err(|e| SearchError::RequestFailed(e.to_string()))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SearchError::ParseFailed(e.to_string()))?;

        let results = body["results"].as_array().ok_or(SearchError::NoResults)?;

        json_results_nonempty(results, "content")
    }
}

// ── SearchProvider enum ──────────────────────────────────

#[derive(Clone)]
pub enum SearchProvider {
    Brave(BraveSearch),
    SearXNG(SearXNG),
    DuckDuckGo(DuckDuckGo),
    Tavily(Tavily),
}

impl SearchProvider {
    pub fn name(&self) -> &'static str {
        match self {
            SearchProvider::Brave(_) => "brave",
            SearchProvider::SearXNG(_) => "searxng",
            SearchProvider::DuckDuckGo(_) => "duckduckgo",
            SearchProvider::Tavily(_) => "tavily",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            SearchProvider::Brave(_) => "Brave Search",
            SearchProvider::SearXNG(_) => "SearXNG (self-hosted)",
            SearchProvider::DuckDuckGo(_) => "DuckDuckGo",
            SearchProvider::Tavily(_) => "Tavily",
        }
    }

    pub fn is_configured(&self) -> bool {
        match self {
            SearchProvider::Brave(s) => s.is_configured(),
            SearchProvider::SearXNG(s) => s.is_configured(),
            SearchProvider::DuckDuckGo(s) => s.is_configured(),
            SearchProvider::Tavily(s) => s.is_configured(),
        }
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        match self {
            SearchProvider::Brave(s) => s.search(query).await,
            SearchProvider::SearXNG(s) => s.search(query).await,
            SearchProvider::DuckDuckGo(s) => s.search(query).await,
            SearchProvider::Tavily(s) => s.search(query).await,
        }
    }
}

// ── Provider factory ─────────────────────────────────────

pub fn provider_from_config(cfg: &super::types::AiConfig) -> SearchProvider {
    match cfg.search_provider.as_str() {
        "searxng" => SearchProvider::SearXNG(SearXNG::new(cfg.searxng_url.clone())),
        "duckduckgo" => SearchProvider::DuckDuckGo(DuckDuckGo::new()),
        "tavily" => SearchProvider::Tavily(Tavily::new(cfg.tavily_api_key.clone())),
        _ => SearchProvider::Brave(BraveSearch::new(cfg.brave_api_key.clone())),
    }
}

// ── Context formatter ────────────────────────────────────

pub fn format_results_as_context(query: &str, results: &[SearchResult]) -> String {
    let mut ctx = "# Web Search Results\n\n\
         Below are live search results retrieved from the web. \
         Answer the user's question using **only** these results. \
         If the results don't contain enough information, say so. \
         Cite every claim with its source URL.\n\n\
         ## Results\n\n"
        .to_string();
    for (i, r) in results.iter().enumerate() {
        ctx.push_str(&format!(
            "{}. [{}]({})\n   Snippet: {}\n\n",
            i + 1,
            r.title,
            r.url,
            r.snippet,
        ));
    }
    ctx.push_str(&format!(
        "## User's Question\n\n{}\n\n\
         ---\n\
         Answer based on the search results above.",
        query,
    ));
    ctx
}
