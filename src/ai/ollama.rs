use crate::ai::types::{AsyncEvent, ChatMessage};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[allow(dead_code)]
    pub model: String,
    pub message: Option<MessageContent>,
    pub done: bool,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    #[allow(dead_code)]
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ListModelsResponse {
    models: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    name: String,
}

#[derive(Clone)]
pub struct OllamaClient {
    base_url: String,
    client: reqwest::Client,
}

impl OllamaClient {
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("Failed to build HTTP client");
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    pub async fn fetch_models(&self, tx: mpsc::UnboundedSender<AsyncEvent>) {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => match resp.json::<ListModelsResponse>().await {
                Ok(data) => {
                    let names: Vec<String> = data.models.into_iter().map(|m| m.name).collect();
                    let _ = tx.send(AsyncEvent::ModelsLoaded(names));
                }
                Err(e) => {
                    let _ = tx.send(AsyncEvent::ModelsError(format!("Parse error: {e}")));
                }
            },
            Err(e) => {
                let _ = tx.send(AsyncEvent::ModelsError(format!("Connection error: {e}")));
            }
        }
    }

    pub async fn chat_stream(
        &self,
        model: String,
        messages: Vec<ChatMessage>,
        tx: mpsc::UnboundedSender<AsyncEvent>,
        cancel: Arc<AtomicBool>,
    ) {
        let url = format!("{}/api/chat", self.base_url);
        let body = ChatRequest {
            model,
            messages,
            stream: true,
        };

        let response = match self.client.post(&url).json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(AsyncEvent::StreamError(format!("Request failed: {e}")));
                return;
            }
        };

        let mut buffer = String::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                let _ = tx.send(AsyncEvent::StreamDone);
                return;
            }
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(AsyncEvent::StreamError(format!("Read error: {e}")));
                    return;
                }
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            loop {
                match buffer.find('\n') {
                    Some(nl_pos) => {
                        let line = buffer[..nl_pos].trim().to_string();
                        buffer = buffer[nl_pos + 1..].to_string();

                        if line.is_empty() {
                            continue;
                        }

                        match serde_json::from_str::<ChatResponse>(&line) {
                            Ok(resp) => {
                                if resp.done {
                                    let _ = tx.send(AsyncEvent::StreamDone);
                                    return;
                                }
                                if let Some(msg) = resp.message {
                                    if !msg.content.is_empty() {
                                        let _ = tx.send(AsyncEvent::StreamChunk(msg.content));
                                    }
                                }
                            }
                            Err(e) => {
                                let _ =
                                    tx.send(AsyncEvent::StreamError(format!("Parse error: {e}")));
                                return;
                            }
                        }
                    }
                    None => break,
                }
            }
        }
        let _ = tx.send(AsyncEvent::StreamDone);
    }
}
