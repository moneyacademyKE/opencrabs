# ADR-0005: Evaluation and Rejection of Bevy UI, GPUI, and Iced

## Context & Problem Statement
Evaluated pure GPU native rendering engines (Bevy UI, GPUI, Iced) vs our Tauri 2 webview container architecture.

## Evaluation Summary
- **Bevy UI**: Complects game engine ECS schedules with simple desktop document layout. No native text selection or copy-paste out of the box. Re-inventing markdown streaming requires custom game shaders.
- **GPUI (Zed Engine)**: Exceptional 120 FPS Metal text renderer, but Mac-first origins cause Windows/Linux platform drift. High API churn and custom context handles (`cx.notify()`).
- **Iced**: Robust Elm architecture, but monolithic `Message` enums produce excessive boilerplate as UI scales across 15+ sub-panels.

## Decision Outcome
**Reject Bevy UI, GPUI, and Iced** for OpenCrabs Desktop GUI. Reaffirm Tauri 2 container with Dioxus WASM as the optimal architecture.

## Consequences
- Saves 30+ hours of custom text layout & markdown renderer development.
- Preserves native OS CSS3 Flexbox/Grid support.
