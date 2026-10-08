use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub ollama_url: String,
    pub search_provider: String,
    pub brave_api_key: String,
    pub tavily_api_key: String,
    pub searxng_url: String,
    pub search_enabled_by_default: bool,
    pub max_notes: usize,
    pub max_history: usize,
    pub notes_context_enabled: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".to_string(),
            search_provider: "duckduckgo".to_string(),
            brave_api_key: String::new(),
            tavily_api_key: String::new(),
            searxng_url: "http://localhost:8888".to_string(),
            search_enabled_by_default: false,
            max_notes: 3,
            max_history: 20,
            notes_context_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: u64,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub model: String,
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub notes_context: bool,
}

#[derive(Debug)]
pub enum AsyncEvent {
    ModelsLoaded(Vec<String>),
    ModelsError(String),
    StreamChunk(String),
    StreamDone,
    StreamError(String),
    SearchResults(Vec<super::search::SearchResult>),
    SearchError(String),
}
