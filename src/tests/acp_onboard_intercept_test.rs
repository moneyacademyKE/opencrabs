//! The ACP `/onboard` intercept: onboarding prompts answer out-of-band
//! (no model turn, no permission ask), everything else falls through to a
//! normal turn. The family is matched exactly like the command path does —
//! lowercase heads, whitespace-trimmed — and arg-bearing steps must stay on
//! their no-write paths here (menus/help text only; config writes are
//! exercised manually, like the channel tests).

use crate::acp::server::AcpServer;

fn reply(text: &str) -> Option<String> {
    AcpServer::onboard_reply(text)
}

#[test]
fn bare_onboard_serves_the_menu() {
    let r = reply("/onboard").expect("bare /onboard must intercept");
    assert!(r.contains("channels"), "{r}");
    assert!(r.contains("/models"), "{r}");
}

#[test]
fn step_prompts_route_with_args() {
    let r = reply("/onboard:channels telegram").expect("onboard:channels intercepts");
    assert!(
        r.to_lowercase().contains("botfather"),
        "no-token arm is help text, no config write: {r}"
    );
}

#[test]
fn provider_step_names_models() {
    let r = reply("/onboard:provider").expect("provider intercepts");
    assert!(r.contains("/models"), "{r}");
}

#[test]
fn unknown_step_returns_the_error_text() {
    let r = reply("/onboard:bogus")
        .expect("unknown steps still intercept — honest error beats a model burn");
    assert!(r.contains("bogus"), "{r}");
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    let r = reply("  /onboard:image  ").expect("trimmed head matches");
    assert!(r.to_lowercase().contains("gemini"), "{r}");
}

#[test]
fn non_onboard_prompts_fall_through() {
    assert!(reply("/models x").is_none());
    assert!(reply("/onboarding-wizard").is_none());
    assert!(reply("/compact").is_none());
    assert!(reply("fix the login bug").is_none());
    assert!(reply("").is_none());
}
