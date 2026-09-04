# ADR-0003: Fork-aware CI and release pipelines

- **Status:** Accepted
- **Date:** 2026-09-04 (PRs #6, #8; runner-label fix `b02ca62a`)
- **Deciders:** moe (owner), opencrabs (implementing agent)

## Context

Upstream's workflows encode two assumptions the fork cannot satisfy:

1. **Runners:** upstream routes push-event Lint/Audit — and all its darwin
   release builds — to its own `self-hosted` machines. The fork has no
   self-hosted runners; inherited labels wedge jobs in `queued` forever
   (observed: 7 hours, zero jobs started, after the 2026-09-04 sync).
2. **crates.io:** the release workflow publishes the `opencrabs` crate and
   couples release creation to publish success ("all-or-nothing"). The fork
   can never publish there — upstream owns the crate name and fork secrets
   do not include a registry token — so the coupled design made every fork
   tag die after builds succeeded.

## Decision

1. **Hosted runners only on push-path jobs.** Lint and Cargo Audit run on
   `ubuntu-latest`. Step-level `if: runner.environment == 'self-hosted'`
   guards are kept (they no-op on hosted runners). The PR-only build matrix
   is darwin-only on `macos-latest`; its `os: self-hosted` entry stays
   push-gated off and is named in FORK_INTENTS.
2. **Publish skips when unconfigured.** The crates.io publish step's `if`
   checks `CARGO_REGISTRY_TOKEN` (on the STEP — job-level `if` rejects the
   `env` context and invalidates the whole workflow; observed live) and
   skips-and-succeeds when unset. `create-release` keeps upstream's
   all-or-nothing `needs`, which is now satisfiable on the fork.
3. **macOS-only release matrix.** Linux/Windows release binaries are a
   non-goal (ADR-0001). The `SHA256SUMS` job uses wildcards and shrinks
   automatically.
4. **Every workflow edit passes `actionlint` locally before push.** GitHub's
   only signal for an invalid workflow is an opaque zero-jobs failure run.
5. Version bumps ride the same commits as train changes so binaries always
   report their tag's version (the 0.3.81-in-v0.3.84 skew cost a full retag).

## Consequences

- Releases are born mac-only with correct versions from v0.3.85 onward.
- Each upstream sync must re-apply these intents (FORK_INTENTS #3–#5);
  upstream may rename or restructure the workflows — re-express at the new
  home.
- Upstream's all-or-nothing crates guarantee is intentionally preserved for
  any future environment where a token exists.
