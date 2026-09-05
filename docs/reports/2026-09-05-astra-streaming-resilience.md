# OpenCrabs streaming resilience for bounded-context Astra routes

**Date:** 2026-09-05  
**Bankai:** `bk-bd79`  
**Branch:** `fix/stream-bounded-context`  
**Scope:** OpenCrabs only. Theseus, Eileen, Axiom, credentials, and deployment were untouched.

## Executive finding

Two failures were complected:

1. **Transport failure:** the Tupesa alias `infered/astra-budget` repeatedly ended an SSE stream without a terminal event. OpenCrabs correctly classified this as a dropped stream.
2. **Request-budget pressure:** OpenCrabs reserved the global `65,536` output-token ceiling even when the selected provider was intentionally capped at a `200,000`-token context. That left only about `134,464` tokens for input, hidden reasoning, and tool schemas. The affected session then entered compaction on nearly every tool round.

The smallest useful fix is not an Astra branch or another retry ladder. Output reservation is now derived from the active session window and capped at 20% of it: `40,000` tokens on a 200K route and the existing `65,536` ceiling on a 1M route.

## Evidence

### Live ledger

Read-only queries against `feedback_ledger` found:

- **9** `tupesa/infered/astra-budget` stream failures on 2026-09-05.
- The repeated fingerprint was `Stream ended without [DONE]: 1 content blocks, 0 output tokens — connection likely dropped`.
- **61** Astra-context compaction events between 12:22 and 13:21 UTC, repeatedly at roughly **130K–160K tokens**.
- A separate fingerprint, `error decoding response body`, also occurred on the direct Infer route. It is a transport/decode failure, not proof that Astra itself generated malformed content.

The two fingerprints must remain distinct from ordinary HTTP 502/503 responses and the 60-second stream-handshake timeout.

### Instrumented Astra sub-agent

A read-only child was run on `tupesa/infered/astra-budget`:

- Session: `b7a6e032-1b94-4c58-8e41-88ce39808fcc`
- It emitted **5 real structured tool calls** (`read_file` ×3, `grep` ×2).
- It completed naturally, without a narration-only loop or timeout.
- Two dropped-stream ledger entries landed around the probe window, so the route is intermittent rather than incapable of tools.

This matters: a successful short probe disproves “Astra cannot call tools.” It does not disprove instability under long, high-context turns.

## Existing stream path

The inspected call path is:

1. `src/brain/provider/custom_openai_compatible.rs` encodes Chat Completions and parses SSE.
2. `src/brain/agent/service/helpers.rs` accumulates stream events, applies handshake/idle limits, and rejects missing terminal state unless complete text can be proven safe.
3. `src/brain/agent/service/tool_loop.rs` retries stream/timeout failures and then walks the configured fallback chain.
4. `src/brain/agent/service/compaction.rs` triggers compaction at 65%, applies backpressure at 80%, and hard-truncates at 90%.
5. TUI/channel progress receives retry and provider-switch events.

The transport code already has TCP keepalive, a 60-second cloud handshake bound, a 90-second remote stream-idle bound, cancellation-aware backoff, fallback, and conservative missing-`[DONE]` handling. Adding yet another retry loop would multiply latency without addressing the context pressure.

## Change

### New policy module

`src/brain/agent/service/request_budget.rs` contains one pure function:

- unknown/zero window → preserve configured output cap;
- known window → `min(configured_max, context_window × 20%)`.

### Application points

`request_max_tokens_for_session(session_id)` now feeds:

- initial requests;
- ordinary tool-loop requests;
- stream retries;
- provider fallbacks;
- context-length recovery;
- empty-answer fallback attempts;
- synchronous and background compaction requests.

There are no remaining `.with_max_tokens(self.max_tokens)` request builders under `src/brain/agent/service/`.

## Verification

| Command | Result |
|---|---|
| `cargo test --all-features --lib request_budget_test` | 4 passed, exit 0 |
| Targeted context/stream/provider regressions | 41 passed, exit 0 |
| `cargo clippy --all-features --lib -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --all-features --lib` | 7,592 passed, 30 ignored, exit 0 |
| `wc -l src/brain/agent/service/request_budget.rs src/tests/request_budget_test.rs` | 23 + 30 LOC |

The first Cargo attempt failed because the OpenCrabs shell had neither Cargo nor CMake on its narrow `PATH`; verification was rerun with the installed persistent toolchain paths. No dependency or system package was installed.

## Official GPT-6 Astra peculiarities

The Tupesa name `infered/astra-budget` is a router alias. Public OpenAI documentation describes the upstream model as `gpt-6-astra`; alias behavior, fallback routing, and any gateway truncation remain Tupesa concerns.

### Model and API behavior

- Public model capacity is **1,050,000 total context**, **922,000 maximum input**, and **128,000 maximum output**. A locally configured 200K route is therefore an intentional harness budget, not Astra's public model limit.
- Astra supports `low`, `medium`, `high`, `xhigh`, and `max` reasoning effort, but **not `none`**. Higher effort trades latency and output-token spend for quality; OpenAI says `xhigh` should be used only when evals justify it.
- Official guidance recommends the **Responses API**. Chat Completions remains supported for text streaming, but the reasoning guide says Astra function calling should use Responses.
- Responses streaming has semantic terminal events such as `response.completed`, `response.failed`, and `error`. OpenCrabs currently consumes Chat Completions-style SSE (`finish_reason` / `[DONE]`), so it cannot use Astra's newer async-tool, mid-turn steering, `configuration_update`, or persisted-response semantics through this provider path.
- Astra supports async tool calling, WebSocket mid-turn steering, and changing reasoning effort via `configuration_update` without invalidating the stable prompt prefix. OpenCrabs' present Chat Completions path serializes tool rounds synchronously.
- Unsupported request controls include `temperature`, `top_p`, and log-probability fields. OpenCrabs only sends temperature when explicitly set, which is compatible with the current default request path.
- Prompt caching is enabled on supported first-party models, but GPT-5.6-and-later cache writes cost 1.25× input and exact prefix stability matters. OpenCrabs deliberately keeps its system prompt as one string because prior multipart-system experiments broke tools on several OpenAI-compatible gateways. Do not trade working tools for a speculative cache optimization.
- Astra tends to ask clarifying questions, can be highly sensitive to conflicting `AGENTS.md`/skill instructions, may delegate less than desired, tends toward formatted/detail-heavy prose, and may over-test small changes. These are model-card prompting concerns, not stream parser bugs.
- Misalignment monitoring is asynchronous and may add friction to tool-using trajectories. It should not be confused with a local OpenCrabs timeout unless the provider returns a corresponding error.

## Prompt-scan verdict

The official docs contain agent-directed prompt examples. Scan result: **CONDITIONAL / low risk**. No exfiltration, PII, encoded payload, or executable side effect was found. The examples are documentation, not instructions to this agent. Copying them wholesale would conflict with local approval and hierarchy rules, so only factual model behavior was used.

## Remaining limitations

1. **No live before/after long-context trial yet.** This source change is verified offline; the running binary was not rebuilt because the user did not explicitly request an OpenCrabs rebuild.
2. **Router observability is incomplete.** OpenCrabs records the alias, not which member of the `astra-budget` cascade produced or dropped a stream. A gateway request ID or resolved-upstream header would make future attribution precise.
3. **Chat Completions remains a compatibility lane.** A future Responses-provider implementation could gain typed terminal events, async tool results, mid-turn steering, and response continuity, but that is a separate API-boundary project—not part of this minimal fix.
4. **The 20% cap is a conservative harness policy.** It is covered by tests and fixes the observed 200K geometry, but should be revisited only with workload measurements, not model-launch hype.

## Rich Hickey certification

The change separates transport completion from request budgeting, adds one pure arithmetic rule, and applies it uniformly. It rejects the complecting alternative—Astra-specific retries and aliases inside the stream parser. The remaining transport fault stays observable and honest; the harness stops manufacturing avoidable context pressure around it.

## Sources

- OpenAI GPT-6 Astra model reference: https://developers.openai.com/api/docs/models/gpt-6-astra.md
- OpenAI latest-model/Astra guidance: https://developers.openai.com/api/docs/guides/latest-model.md
- OpenAI streaming guide: https://developers.openai.com/api/docs/guides/streaming-responses.md
- OpenAI reasoning guide: https://developers.openai.com/api/docs/guides/reasoning.md
- OpenAI prompt-caching guide: https://developers.openai.com/api/docs/guides/prompt-caching.md
