//! Headless `/onboard` intercept for ACP prompts.
//!
//! Channels and the TUI answer the onboarding command family before the
//! model ever sees it; ACP used to feed those prompts to the LLM, which
//! burned minutes and a permission dialog to say what a static menu says
//! instantly. This module matches the family and routes it to the same
//! [`crate::brain::tools::slash_onboard`] handlers chat uses — one dispatch,
//! every surface.

/// Match `/onboard` and `/onboard:<step>` prompt text and dispatch it.
/// Returns the reply text, or `None` when the prompt is not onboarding and
/// should run as a normal model turn.
///
/// Heads are matched lowercase, like the rest of the command path; the step
/// itself is case-insensitive because `slash_onboard::dispatch` lowercases
/// it. Arg-bearing steps reach their no-write paths here (menus, help text);
/// config-writing shapes are exercised manually, like the channel tests.
pub(crate) fn reply(text: &str) -> Option<String> {
    let text = text.trim();
    let (head, args) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let step = if head == "/onboard" {
        ""
    } else {
        head.strip_prefix("/onboard:")?
    };
    let result = crate::brain::tools::slash_onboard::dispatch(step, args).ok()?;
    Some(if result.success {
        result.output
    } else {
        result
            .error
            .unwrap_or_else(|| "Onboarding dispatch failed.".into())
    })
}
