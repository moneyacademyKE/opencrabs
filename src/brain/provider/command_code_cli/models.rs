//! Canonical model definitions for Command Code CLI.

/// Canonical model list for Command Code CLI, mirroring `command-code --list-models` (v1.4.1).
/// The CLI validates whatever the account can access and accepts either the full id or short name.
pub const SUPPORTED_MODELS: &[&str] = &[
    // Open Source
    "deepseek/deepseek-v4-pro",
    "deepseek/deepseek-v4-flash",
    "moonshotai/kimi-k3",
    "moonshotai/kimi-k2.7-code",
    "moonshotai/kimi-k2.7-code-highspeed",
    "moonshotai/kimi-k2.6",
    "moonshotai/kimi-k2.5",
    "zai-org/glm-5.2",
    "zai-org/glm-5.2-fast",
    "zai-org/glm-5.1",
    "zai-org/glm-5",
    "minimaxai/minimax-m3",
    "minimaxai/minimax-m2.7",
    "minimaxai/minimax-m2.5",
    "xiaomi/mimo-v2.5-pro",
    "xiaomi/mimo-v2.5",
    "qwen/qwen3.6-max-preview",
    "qwen/qwen3.6-plus",
    "qwen/qwen3.7-max",
    "qwen/qwen3.7-plus",
    "stepfun/step-3.7-flash",
    "stepfun/step-3.5-flash",
    "tencent/hy3-paid",
    "nvidia/nemotron-3-ultra-550b-a55b",
    "thinkingmachines/inkling",
    "poolside/laguna-s-2.1-free",
    "inclusionai/ling-3.0-flash-free",
    // Anthropic
    "claude-sonnet-5",
    "claude-sonnet-4-6",
    "claude-fable-5",
    "claude-opus-5",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-haiku-4-5",
    // OpenAI
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.3-codex",
    "gpt-5.4-mini",
    // Google
    "google/gemini-3.6-flash",
    "google/gemini-3.5-flash",
    "google/gemini-3.5-flash-lite",
    "google/gemini-3.1-flash-lite",
    // Sakana
    "sakana/fugu-ultra",
    // Meta
    "meta/muse-spark-1.1",
    // xAI
    "xai/grok-4.5",
];

/// Default model when no per-session override is set.
pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-flash";
