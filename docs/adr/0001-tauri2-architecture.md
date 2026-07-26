# ADR-0001: Adoption of Tauri 2 Native Container Architecture

## Context & Problem Statement
OpenCrabs requires a native desktop GUI that delivers an "Agent OS" workspace experience (similar to Hermes Desktop) with rich chat streaming, tool execution, multi-channel control, cron management, and brain editing.

Traditional Electron app wrappers require embedding a full Chromium instance and Node.js runtime, producing heavy binaries (>120MB) and high idle memory consumption (>150MB RAM).

## Decision Drivers
- Ultra-low memory footprint (<20MB idle RAM).
- Fast cold boot time (<200ms).
- High security isolation without exposing local network ports.
- In-process integration with the existing `opencrabs` Rust backend crate.

## Considered Options
1. **Electron Wrapper**: Standard JS/Node desktop container.
2. **Tauri 2**: Lightweight native WebKit/WebView2 container with Rust host bindings.
3. **Custom Winit + Skia Window**: Custom native window renderer.

## Decision Outcome
Chosen option: **Tauri 2**, because it embeds the host OS's native Webview (WebKit on macOS, WebView2 on Windows, WebKitGTK on Linux), resulting in a <15MB binary size and ~15MB RAM idle consumption.

## Consequences
- **Positive**: Direct in-process access to `AgentService` and `ServiceContext`. Sub-millisecond IPC.
- **Positive**: Embedded static assets with zero external web server requirement.
- **Negative**: Relies on host OS Webview rendering quirks across operating systems.
