//! Command Code CLI Provider — direct subprocess integration
//!
//! Thin metadata provider for the `command-code` subprocess backend. This keeps
//! the core crate compiling and exposes the provider surface used by model
//! menus, onboarding, and factory wiring. The actual CLI execution path can be
//! filled in later without breaking the desktop or the main app build.

mod models;

pub use models::{DEFAULT_MODEL, SUPPORTED_MODELS};

use super::error::{ProviderError, Result};
use super::r#trait::{Provider, ProviderStream};
use super::types::*;
use async_trait::async_trait;

#[derive(Clone)]
pub struct CommandCodeCliProvider {
    default_model: String,
    configured_context_window: Option<u32>,
}

impl CommandCodeCliProvider {
    pub fn new() -> Result<Self> {
        let _path = resolve_command_code_path()?;
        Ok(Self {
            default_model: DEFAULT_MODEL.to_string(),
            configured_context_window: None,
        })
    }

    pub fn with_context_window(mut self, context_window: u32) -> Self {
        self.configured_context_window = Some(context_window);
        self
    }

    pub fn with_default_model(mut self, model: String) -> Self {
        self.default_model = model;
        self
    }
}

fn resolve_command_code_path() -> Result<String> {
    if let Ok(path) = std::env::var("COMMAND_CODE_PATH") {
        if std::path::Path::new(&path).exists() {
            return Ok(path);
        }
        return Err(ProviderError::Internal(format!(
            "COMMAND_CODE_PATH set but not found: {}",
            path
        )));
    }

    let home = dirs::home_dir().unwrap_or_default();
    let candidates = [
        std::path::PathBuf::from("/opt/homebrew/bin/command-code"),
        std::path::PathBuf::from("/usr/local/bin/command-code"),
        home.join(".local/bin/command-code"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    if let Some(path) = super::which_binary("command-code") {
        return Ok(path);
    }

    Err(ProviderError::Internal(
        "command-code CLI not found — install it or set COMMAND_CODE_PATH".to_string(),
    ))
}

#[async_trait]
impl Provider for CommandCodeCliProvider {
    async fn complete(&self, _request: LLMRequest) -> Result<LLMResponse> {
        Err(ProviderError::Internal(
            "command-code CLI execution path is not implemented yet".to_string(),
        ))
    }

    async fn stream(&self, _request: LLMRequest) -> Result<ProviderStream> {
        Err(ProviderError::Internal(
            "command-code CLI streaming path is not implemented yet".to_string(),
        ))
    }

    fn name(&self) -> &str {
        "command-code-cli"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn supported_models(&self) -> Vec<String> {
        SUPPORTED_MODELS.iter().map(|m| (*m).to_string()).collect()
    }

    fn supports_vision(&self) -> bool {
        false
    }

    fn cli_handles_tools(&self) -> bool {
        true
    }

    fn cli_manages_context(&self) -> bool {
        false
    }

    fn context_window(&self, _model: &str) -> Option<u32> {
        self.configured_context_window.or(Some(128_000))
    }

    fn configured_context_window(&self) -> Option<u32> {
        self.configured_context_window
    }

    fn calculate_cost(&self, _model: &str, _input_tokens: u32, _output_tokens: u32) -> f64 {
        0.0
    }
}
