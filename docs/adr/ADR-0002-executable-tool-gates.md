# ADR-0002: Executable tool gates — policy as data

- **Status:** Accepted
- **Date:** 2026-09-04 (implemented in fork PR #3; ported onto upstream main in the 2026-09-04 sync)
- **Deciders:** moe (owner), opencrabs (implementing agent)

## Context

Agent safety rules previously lived as prose (AGENTS.md hard rules). Prose
enforcement depends on the model re-reading and re-applying rules every turn
— it decays under context compaction and has no mechanical guarantee.
A sibling runtime (Theseus) demonstrated enforcement via sci-evaluated
predicate rules: `tool + args → :allow/:deny`, first match wins,
deny-by-default, evaluated in the tool path before execution.

## Decision

Port the **semantics**, not the machinery:

- Gates are a **declarative TOML table** (`gates.toml`, shipped as
  `gates.toml.example`), not an embedded eval engine. Matchers are
  tool-name + regex-shaped argument patterns; decisions are
  `deny` / `allow` / `prompt`.
- **First match wins; fail-closed** — every fault class (missing file, bad
  regex, unreadable entry, evaluator error) resolves to `prompt`, never to
  silent allow.
- Evaluated in the tool dispatch path **before** the approval computation,
  in both dispatch modes: the parallel path treats `deny`/`prompt` gates as
  parallel-ineligible so batching cannot bypass a gate.
- `allow` only pre-clears a prompt. It can never turn a denied action into
  silence, and irreversible actions always prompt regardless of gates.
- The shipped example constitution is **pinned byte-for-byte by tests**
  (`include_str!`), so the file that ships is the file that is tested.

Rejected: embedding a script interpreter (sci-style) in Rust to evaluate
regex-shaped rules — incidental complexity; a data table needs data
evaluation.

## Consequences

- Rules are mechanically enforced once an instance runs a build containing
  this commit (≥ v0.3.84 fork releases).
- Adding a gate class is one table row; no code change.
- `gates.toml` is profile-aware (`<opencrabs_home>/gates.toml`), following
  the config.toml convention.
- If upstream grows a competing mechanism, prefer upstream's and delete ours
  (per ADR-0001 intent #2).
