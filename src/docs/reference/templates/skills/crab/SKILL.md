---
name: crab
description: Install, inspect, list, remove, and update-check crabs — portable skill packs from the Crab Market. Drives the native `opencrabs crab` CLI. Triggers on: crab, crabs, skill pack, marketplace, crab market, install skill, tier 2.
review_gate: true
---

# Crab Market Skill

Crabs are portable skill packs: a `crab.toml` manifest plus skill dirs, installed into
`~/.opencrabs/skills/` by the native installer. This skill drives the `opencrabs crab` CLI.

## Hard rule: inspect before install, always

An install is only ever proposed AFTER the owner has seen the inspection report —
files, frontmatter, blast radius, secret scan. Never run `opencrabs crab install --yes`
unless the owner has already approved THAT crab (by name and pin) in this conversation.

## Commands

- `opencrabs crab inspect <source>` — pre-approval report. `<source>` is a git URL or a
  local dir containing `crab.toml`. Output is archived under `state/crab-inspections/`.
- `opencrabs crab install <source> [--pin <sha>] [--yes] [--force]` — inspect → prompt →
  copy → pin. `--yes` skips the prompt (owner-approved installs only). `--force` re-approves
  collisions/upstream drift after re-inspection. Exit codes: 3 = secrets found (refused),
  4 = collision or drift (needs `--force`), 2 = manifest/clone failure, 1 = not found.
- `opencrabs crab list` — installed crabs with pins.
- `opencrabs crab remove <name>` — deletes exactly the ledger-recorded files.

## Agent workflow (natural-language request → action)

1. User names a crab or a source → run `opencrabs crab inspect <source>` FIRST, and show
   them the report (or summarize: file count, skills, blast radius, secret verdict).
2. WAIT for explicit approval. Do not chain install into the same turn.
3. On approval → `opencrabs crab install <source> --yes`. Never paper over exit 3/4 with
   `--force` without a fresh inspection and a fresh "yes".
4. Removal → confirm the name matches `opencrabs crab list`, then run remove.

## What a crab looks like

```toml
# crab.toml
name = "competitor-watch"
version = "0.1.0"
description = "Daily page-diff over watched URLs; reports only changes"

[[skills]]
path = "skills/competitor-watch"
```

Skill dirs must live under `skills/` and contain `SKILL.md` with valid frontmatter
(`name`, `description`). The manifest is the allowlist — exactly the declared skill
dirs are installed, nothing else.
