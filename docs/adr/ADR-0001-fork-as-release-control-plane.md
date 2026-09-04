# ADR-0001: Fork as release-control plane over adolfousier/opencrabs

- **Status:** Accepted
- **Date:** 2026-09-04
- **Deciders:** moe (owner), opencrabs (implementing agent)

## Context

OpenCrabs upstream (`adolfousier/opencrabs`) is the source of truth for the
agent runtime this deployment executes — 500+ commits ahead of the fork's
original base and moving daily. The deployment is single-tenant, macOS
arm64-only, and needs capabilities upstream does not carry (executable tool
gates) plus control over its own update train (`/evolve`, `/rebuild`,
crash-recovery version checks — all compile-time constants, no config or env
override exists).

## Decision

1. The fork tracks upstream `main` as its base. Fork divergence stays a
   **minimal, named list** (`.github/FORK_INTENTS.md`) re-applied at every
   sync — never a growing patch mountain.
2. The update train points at the fork: `/evolve` and `/rebuild` consume
   fork releases; crash-recovery checks the fork's latest release.
3. Releases are fork-tagged and versioned **strictly ahead** of upstream's
   latest (upstream 0.3.83 → fork 0.3.85+), so semver comparison in
   `/evolve` can never point an installed binary back at upstream.
4. Shipping targets are **macOS amd64 + arm64 only**. Linux and Windows
   release binaries are a non-goal; Linux runners remain the CI *test gate*.
5. Upstream features the fork needs are contributed upstream where possible
   (gates candidate); fork-local copies are deleted if upstream adopts them.

## Consequences

- Sync is a recurring, measured procedure (dry-run replay, conflict
  classification, backup branch, CI-toolchain gates) — see
  `.github/FORK_PLAYBOOK.md` §1. Cost grows with drift; sync before feature
  work.
- Every sync re-applies intents at homes that may have moved (upstream
  refactors aggressively: module splits, file renames).
- The fork can never publish to crates.io (name owned upstream) — see
  ADR-0003.
- No proprietary content blocks this: the fork is public; divergence is
  process, not secrets.
