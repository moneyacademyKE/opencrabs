# Desktop Command Contract

Status legend:
- **ready** — implemented and suitable for normal desktop use
- **partial** — works, but with clear limitations or truthfulness gaps
- **unsupported** — exposed today but should be hidden/gated until real

## Sessions / Chat

| Command | Status | Notes |
|---|---|---|
| `list_sessions` | ready | Lists sessions from the service manager. |
| `create_session` | ready | Creates a session and returns summary info. |
| `rename_session` | ready | Renames an existing session. |
| `delete_session` | ready | Deletes a session. Destructive; UI should keep confirmation. |
| `get_session_messages` | ready | Reads message history for a session. |
| `send_message` | ready | Non-streaming message send path exists. |
| `send_message_streaming` | ready | Desktop subscribes to Tauri stream events, renders token deltas live, and reloads canonical persisted messages on completion. |
| `stop_generation` | partial | Desktop exposes Stop and clears its live stream state on cancellation; provider-specific cancellation timing remains asynchronous. |

## Config / Providers / Onboarding

| Command | Status | Notes |
|---|---|---|
| `get_config` | ready | Returns app config summary for desktop panels. |
| `get_providers` | ready | Provider list available for provider UI. |
| `select_model` | ready | Desktop model selection persists the change and refreshes the provider summary. |
| `update_config` | partial | Now guarded, but desktop still needs a strict UI-side field policy and clearer error handling. |
| `is_first_time_setup` | ready | Simple setup detection. |
| `get_available_providers` | ready | Returns onboarding provider catalog. |
| `validate_api_key` | partial | No longer blindly writes keys, but validation is heuristic/shape-based rather than live provider verification. |
| `save_onboarding_config` | partial | Writes onboarding config, but deserves stronger UX review around secrets and provider validation feedback. |
| `run_health_check` | partial | More honest than before, but still not a full operational health model. |

## Brain / Files

| Command | Status | Notes |
|---|---|---|
| `list_brain_files` | ready | Lists allowlisted brain files. |
| `read_brain_file` | ready | Reads allowlisted brain files. |
| `write_brain_file` | partial | Guarded better now, but remains a privileged edit surface that needs stronger UI constraints. |
| `list_directory` | partial | Contained to workspace root, but workspace-root model still needs explicit production semantics. |
| `read_file_content` | partial | Constrained, but still tied to current workspace-root assumptions. |
| `get_workspace_root` | partial | Returns current desktop workspace root, but root resolution policy should be made explicit and persistent. |

## Tools / Skills / Dynamic Tools

| Command | Status | Notes |
|---|---|---|
| `list_tools` | ready | Lists core + dynamic tools. |
| `get_tool_details` | ready | Returns detailed tool metadata. |
| `approve_tool` | partial | Desktop can persist an approval decision for the selected tool/session, but does not yet mirror the TUI's richer inline approval event workflow. |
| `list_skills` | ready | Lists skills with disabled-state awareness. |
| `get_skill_details` | ready | Returns skill details. |
| `toggle_skill` | ready | Desktop toggles the persisted state and refreshes both list and selected detail. |
| `list_dynamic_tools` | ready | Lists dynamic tools from `tools.toml`. |
| `add_dynamic_tool` | partial | Powerful mutation surface; production UI should gate and validate more aggressively. |
| `remove_dynamic_tool` | partial | Same mutation concern as add. |

## Cron / Channels / Usage / Panes / Voice / Updates

| Command | Status | Notes |
|---|---|---|
| `list_cron_jobs` | ready | Lists configured cron jobs. |
| `create_cron_job` | partial | Works, but still needs stronger desktop validation UX and confirmation. |
| `delete_cron_job` | partial | Destructive; should remain confirmation-gated. |
| `toggle_cron_job` | ready | Desktop exposes enable/disable and refreshes the job list. |
| `trigger_cron_job` | ready | Desktop exposes manual trigger. |
| `list_cron_runs` | ready | Lists cron run history; run-history UI is not implemented yet. |
| `get_channel_statuses` | partial | Reports enablement plus configuration/credential readiness. It does not claim a live connection. |
| `toggle_channel` | ready | Desktop persists the change and refreshes channel status; runtime reconnect/reload is outside this command. |
| `get_usage_data` | ready | Usage data is available for dashboard views. |
| `get_pane_layout` | ready | Persists and restores the full split tree, focus, and pane/session mappings. |
| `split_pane` | ready | Updates the persisted split tree. |
| `close_pane` | ready | Compacts the persisted split tree and clears the closed pane mapping. |
| `set_pane_session` | ready | Persists pane/session mapping across desktop restarts. |
| `get_desktop_state` | ready | Restores the last valid route and selected session, falling back safely. |
| `save_desktop_state` | ready | Validates and persists the active route and selected session. |
| `get_voice_config` | unsupported | Returns an explicit `unsupported` capability state; no voice controls should be shown. |
| `transcribe_audio` | unsupported | Fails explicitly until real STT integration is wired. |
| `synthesize_speech` | unsupported | Fails explicitly until real TTS integration is wired. |
| `check_for_updates` | partial | Honest check path exists, but desktop updater story is not complete. |
| `install_update` | unsupported | Explicitly not wired for desktop-native install yet. |

## UI follow-up notes

The current Dioxus desktop UI exposes the safe interaction paths for model selection, chat streaming/stop, file navigation, brain save, tool approval, skill toggle, cron toggle/manual run, and channel toggle. Remaining UI work:

1. Hide or mark as unavailable:
   - voice panel/actions until real STT/TTS exists
   - install-update button/path until updater is real
2. Mark clearly as limited/partial:
   - channel configuration/credential readiness (not liveness)
   - tool approval flow
   - provider-specific cancellation timing
3. Add controls with explicit confirmation for destructive or high-impact operations:
   - session rename/delete
   - cron create/delete and run history
   - dynamic tool removal
   - brain/config writes
