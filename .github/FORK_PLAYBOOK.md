# Fork Playbook — moneyacademyKE/opencrabs

Procedures for maintaining this fork as a release-control plane over
`adolfousier/opencrabs`. Companion to [FORK_INTENTS.md](FORK_INTENTS.md) —
that file is the *what* (the named divergence checklist); this file is the
*how* (procedures, gates, and the failure modes already paid for).
Decisions live in `docs/adr/`.

## Standing rule

Every step ends in a receipt that was actually observed (command exit, API
response, file bytes). A step whose receipt never came back did not happen —
re-verify, never re-do blindly.

## 1. Upstream sync

Run when upstream drifts past ~100 commits or before fork-side feature work.

1. **Measure, never re-derive.** `git fetch origin` (upstream), then count
   both directions (`rev-list --count` on each side of the fork base) and
   list files changed on both sides since the merge-base.
2. **Dry-run the replay** of each intent commit onto `origin/main` in a
   scratch worktree. Classify every conflict before resolving anything:
   - **Dissolve** — upstream already fixed it (lint ports, lockfile bumps).
     Take upstream's side; drop our hunk.
   - **Re-express** — upstream moved the seam (module splits, renames are
     common: `evolve/mod.rs` → submodules, `self_update.rs` → `src/brain/`).
     Keep upstream's structure; re-apply the intent at its new home.
   - **Ours** — gates, update-train constants, workflow intents. Apply clean.
3. **Local gates on CI's exact toolchain** (`cargo +<ci-version> clippy
   --all-targets -- -D warnings`, `fmt --check`, focused test suites). A
   different local toolchain proving nothing is the #1 silent failure — the
   1.97/1.98 gap once hid 13 clippy errors.
4. **Recovery first:** `git branch backup/main-pre-sync-<date>` BEFORE any
   force-land. The force-land is the one irreversible step; make it
   reversible before taking it.
5. **Land, then watch.** A PR opened from the sync branch gets auto-marked
   merged once main contains its head (fine — browsable record). The push
   triggers CI; jobs stuck `queued` with zero started = runner-label leak
   (see §4).
6. **Explain every number.** A test suite returning more tests than the file
   contains means a filter substring-matched another suite. Verify, then ship.

## 2. Release train

1. Tag only a CI-green main tip. Never tag on hope — the tag IS the gate.
2. `git tag vX.Y.Z && git push origin vX.Y.Z` → `release.yml` builds macOS
   amd64/arm64 + `SHA256SUMS` (wildcard job — auto-shrinks with the matrix).
3. Verify the loop, not the intent: run conclusion via API, then
   `releases/latest` returns the new tag, then assets present, then the
   binary *inside* the asset reports the tag's version.
4. `releases/latest` skips drafts and prereleases. During a retag window the
   platform falls back to an older release — harmless (semver never points
   installed binaries backward), but expect it.

## 3. Retag (destructive — explicit owner approval required)

A burned tag (wrong binaries under a correct-looking release) re-offers
itself forever via `/evolve`. Sequence: fixes merged → verify release + tag
exist → `gh release delete <tag> --cleanup-tag` → verify gone (`gh release
view` 404, `git ls-remote` empty) → re-tag on the fixed tip → watch workflow
→ verify `releases/latest` serves the new build. One destructive pass, with
verified before/after states.

## 4. Workflow edits (`ci.yml` / `release.yml`)

1. `actionlint` locally BEFORE pushing. GitHub rejects an invalid workflow
   file with an opaque zero-jobs failure run on the push — cheap to catch
   locally, expensive to diagnose from that.
2. `env` context is illegal in job-level `if:` — condition the STEP instead
   (observed live: job-level use invalidated the whole workflow).
3. Runner labels: hosted only (`ubuntu-latest`, `macos-latest`) on every
   push-path job. Any `self-hosted` label wedges forever — the fork has no
   self-hosted runners; upstream does. Step-level
   `if: runner.environment == 'self-hosted'` guards are fine (they no-op on
   hosted). The PR-only `os: self-hosted` matrix entry in `ci.yml` is
   push-gated off and named in FORK_INTENTS — do not let it reach a push path.
4. crates.io publish is structurally impossible here (upstream owns the crate
   name; forks get no token). The publish step must **skip-and-succeed** and
   must never gate release creation.
5. Watchers log conclusions, never states: a watcher may only write
   `completed/<conclusion>` from the API. `FINAL:in_progress` is a category
   error.

## 5. Local build & binary swap

1. Detached builds need full PATH: rustup's `cargo` AND Homebrew's `cmake`
   (the harness PATH strips Homebrew; llama.cpp needs it).
2. Install with a **fresh inode**: `rm` the old binary, then `cp`. Overwriting
   in place keeps the same inode and its stale signature/xattrs → SIGKILL at
   first exec on macOS, with an identical binary that runs fine elsewhere.
3. Verify: SHA match to the build artifact, fork constants present
   (`strings <bin> | grep moneyacademyKE`), actual execution (`--version`,
   exit 0), old binary backed up first.
4. `cargo clean` before release-grade builds — artifact drift once cost
   210 GiB.

## Patterns (earned — keep doing these)

- **Dry-runs are measurements, not rituals.** A 558-commit sync cost one
  structural surprise because the dry-run classified conflicts first.
- **Port, don't re-derive.** If a fix exists upstream, cherry-pick upstream's
  commit; hand-porting invites twin-site misses (identical-context hunks
  auto-merge once and leave the twin broken — audit the whole diff).
- **Write the script, don't nest the quotes.** `sh -c` has no process
  substitution; heredocs break; quote-stacks collapse. Files execute.
- **Re-query IDs from the source of truth.** A run ID taken from a garbled
  receipt 404'd; fresh `gh run list` corrected it.
- **Fork intent is a named list** (FORK_INTENTS.md). Re-deriving it from
  archaeology each sync is the real cost of divergence.

## Anti-patterns (burned us — don't repeat)

- Trusting a merge tool's success receipt while a concurrent writer reverts
  hunks — audit the full diff after every batch edit.
- `--auto` merge on an unprotected repo: it merges immediately (no required
  checks exist to wait for). If the gate matters, gate the NEXT action on the
  run verdict, not the merge.
- `timeout` on macOS (doesn't exist) and GNU-only flags (BSD userland) —
  `perl -e 'alarm shift; exec @ARGV'` bounds a command portably.
- Letting a test count, runner label, or version string go unexplained.
