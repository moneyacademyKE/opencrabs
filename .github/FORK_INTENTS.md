# Fork Intents — moneyacademyKE/opencrabs

Every sync from `adolfousier/opencrabs` must re-apply these intents after
taking upstream's side of any conflict. This file is the checklist that makes
that mechanical instead of archaeological.

| # | Intent | Where | Notes |
|---|--------|-------|-------|
| 1 | **Update train → fork** | `GITHUB_API` in `src/brain/tools/evolve/release_check.rs`, `REPO_URL` in `src/brain/self_update.rs`, `GITHUB_RELEASES_API` in `src/cli/crash_recovery.rs` | 3 constants → `moneyacademyKE/opencrabs`. Deliberately NOT changed: `agent_card.rs` (A2A metadata), `rsi_sync.rs` (brain templates). `evolve/mod.rs` was split upstream into submodules — the constant lives in `release_check.rs` now |
| 2 | **Executable tool gates** | `src/utils/gates.rs`, `src/utils/gates_test.rs`, `gates.toml.example`, wiring in `tool_loop.rs` + `parallel_tools.rs` | Feature commit (#3). If upstream grows a competing gate mechanism, prefer upstream's and delete ours |
| 3 | **Release train: fork-aware** | `.github/workflows/release.yml` Publish step | Skip (not fail) crates.io publish when `CARGO_REGISTRY_TOKEN` is unset — forks can never publish the upstream crate name. The `if` must stay on the STEP (job-level `if` rejects the `env` context — GitHub invalidates the whole workflow; observed live). `create-release` keeps upstream's all-or-nothing `needs` |
| 4 | **CI runner labels: hosted** | `.github/workflows/ci.yml` ternary labels | `runs-on: ${{ ... && 'ubuntu-latest' \|\| 'self-hosted' }}` → `... \|\| 'ubuntu-latest'` (fork has no self-hosted runners; upstream does). The `if: runner.environment == 'self-hosted'` step guards stay — they no-op on hosted runners |
| 5 | **macOS-only releases** | `release.yml` build matrix + `ci.yml` build matrix | Linux/Windows releases are a non-goal. All darwin builds moved from upstream's `self-hosted` Mac to `macos-latest`. Do NOT remove ubuntu jobs from ci.yml — they are the test gate (lint/test/audit/coverage) |
| 6 | **Version namespace** | `Cargo.toml`, `Cargo.lock` | Fork versions stay strictly ahead of upstream's (upstream at 0.3.83; fork now 0.3.85) so `/evolve` semver never points back at upstream |

## Sync procedure (rebase-style, the pattern that worked 2026-09-04)

1. `git fetch origin` (upstream) and `git fetch fork`
2. Dry-run the rebase of fork intents onto `origin/main` in a scratch worktree — measure conflicts before committing
3. Take upstream's side on shared files; re-apply each intent above at its (possibly moved) home — upstream renames files often (`evolve/mod.rs` → submodules, `self_update.rs` → `src/brain/`)
4. Drop any intent upstream has adopted itself (base-rot commit #4 dissolved this way)
5. Local gates: `cargo +1.98.0 clippy --all-targets -- -D warnings`, `cargo +1.98.0 fmt --check`, `cargo test -p <crate> --lib gates_test` (CI toolchain ≠ local default — the 1.97/1.98 gap hid 13 errors once)
6. Land on main (backup branch first), watch the push-event CI run, then tag
