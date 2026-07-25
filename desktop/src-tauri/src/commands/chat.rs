use crate::AppState;
use opencrabs::brain::agent::AgentService;
use opencrabs::brain::provider::{StreamEvent, create_provider};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, State};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use uuid::Uuid;

#[derive(Clone)]
struct DesktopCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl DesktopCancellationToken {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

static STREAM_CANCELLATIONS: OnceLock<Mutex<HashMap<Uuid, DesktopCancellationToken>>> =
    OnceLock::new();

fn stream_cancellations() -> &'static Mutex<HashMap<Uuid, DesktopCancellationToken>> {
    STREAM_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Serialize)]
pub struct ChatResponse {
    pub message_id: String,
    pub content: String,
    pub model: String,
    pub provider_name: String,
    pub cost: f64,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub tokens_per_second: Option<f64>,
}

#[derive(Serialize, Clone)]
pub struct StreamChunk {
    pub text: String,
}

fn stream_event_text(event: &StreamEvent) -> Option<String> {
    match event {
        StreamEvent::ContentBlockDelta {
            delta: opencrabs::brain::provider::ContentDelta::TextDelta { text },
            ..
        } => Some(text.clone()),
        StreamEvent::ContentBlockDelta {
            delta: opencrabs::brain::provider::ContentDelta::ReasoningDelta { text },
            ..
        } => Some(text.clone()),
        StreamEvent::ContentBlockDelta {
            delta: opencrabs::brain::provider::ContentDelta::ThinkingDelta { thinking },
            ..
        } => Some(thinking.clone()),
        _ => None,
    }
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    model: Option<String>,
) -> Result<ChatResponse, String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;

    let ctx = {
        let sm = state.service_manager.lock().await;
        sm.as_ref()
            .ok_or("Service not initialized")?
            .context()
            .clone()
    };
    let provider = {
        let config = state.config.read().await;
        create_provider(&config).await.map_err(|e| e.to_string())?
    };
    let config = {
        let config_read = state.config.read().await;
        config_read.clone()
    };
    let agent = AgentService::new(provider, ctx, &config).await;
    let response = agent
        .send_message(id, message, model)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ChatResponse {
        message_id: response.message_id.to_string(),
        content: response.content,
        model: response.model,
        provider_name: response.provider_name,
        cost: response.cost,
        input_tokens: response.usage.input_tokens,
        output_tokens: response.usage.output_tokens,
        tokens_per_second: response.tokens_per_second,
    })
}

#[tauri::command]
pub async fn send_message_streaming(
    state: State<'_, AppState>,
    window: tauri::Window,
    session_id: String,
    message: String,
    model: Option<String>,
) -> Result<String, String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;

    let ctx = {
        let sm = state.service_manager.lock().await;
        sm.as_ref()
            .ok_or("Service not initialized")?
            .context()
            .clone()
    };
    let provider = {
        let config = state.config.read().await;
        create_provider(&config).await.map_err(|e| e.to_string())?
    };
    let config = {
        let config_read = state.config.read().await;
        config_read.clone()
    };
    let agent = AgentService::new(provider, ctx, &config).await;

    let mut stream_resp = agent
        .send_message_streaming(id, message, model)
        .await
        .map_err(|e| e.to_string())?;

    let cancel_token = DesktopCancellationToken::new();
    {
        let mut cancellations = stream_cancellations().lock().await;
        if let Some(existing) = cancellations.insert(id, cancel_token.clone()) {
            existing.cancel();
        }
    }

    let message_id = stream_resp.message_id.to_string();
    let mid = message_id.clone();
    let window_clone = window.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                let _ = window_clone.emit(
                    "stream-stopped",
                    serde_json::json!({"session_id": id.to_string(), "message_id": mid}),
                );
                break;
            }

            match stream_resp.stream.next().await {
                Some(Ok(evt)) => {
                    if let Some(text) = stream_event_text(&evt) {
                        let _ = window_clone.emit("stream-chunk", StreamChunk { text });
                    }
                }
                Some(Err(e)) => {
                    let _ = window_clone
                        .emit("stream-error", serde_json::json!({"error": e.to_string()}));
                    break;
                }
                None => {
                    let _ =
                        window_clone.emit("stream-done", serde_json::json!({"message_id": mid}));
                    break;
                }
            }
        }

        let mut cancellations = stream_cancellations().lock().await;
        cancellations.remove(&id);
    });

    Ok(message_id)
}
#[tauri::command]
pub async fn stop_generation(
    _state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let session_uuid = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let cancellations = stream_cancellations().lock().await;
    let Some(token) = cancellations.get(&session_uuid) else {
        return Err(format!("No active stream for session {}", session_id));
    };
    token.cancel();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_event_text_extracts_text_delta() {
        let event = StreamEvent::ContentBlockDelta {
            index: 0,
            delta: opencrabs::brain::provider::ContentDelta::TextDelta {
                text: "hello".to_string(),
            },
        };
        assert_eq!(stream_event_text(&event).as_deref(), Some("hello"));
    }

    #[test]
    fn stream_event_text_extracts_reasoning_and_thinking() {
        let reasoning = StreamEvent::ContentBlockDelta {
            index: 0,
            delta: opencrabs::brain::provider::ContentDelta::ReasoningDelta {
                text: "because".to_string(),
            },
        };
        let thinking = StreamEvent::ContentBlockDelta {
            index: 0,
            delta: opencrabs::brain::provider::ContentDelta::ThinkingDelta {
                thinking: "hmm".to_string(),
            },
        };
        assert_eq!(stream_event_text(&reasoning).as_deref(), Some("because"));
        assert_eq!(stream_event_text(&thinking).as_deref(), Some("hmm"));
    }

    #[test]
    fn cancellation_token_flips_state() {
        let token = DesktopCancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }
}
