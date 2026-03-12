---
name: enforce-mobile-parity
description: Use when implementing or changing any mobile feature that is intended to match existing Windows behavior. Enforces Windows-as-canonical workflow, parity specs under `.claude/parity/`, shared fixtures under `parity-fixtures/`, and no undocumented mobile deviations.
---

# Enforce Mobile Parity

## Overview

Use this skill for Android or future iOS work that is supposed to behave the same as an existing Windows feature.
The default rule is hard parity: mobile follows the Windows behavior contract unless the matching parity spec explicitly documents a deviation.

## Required Workflow

1. Read the canonical Windows implementation first.
2. Update or create the matching spec under `.claude/parity/`.
3. Add or update shared fixtures under `parity-fixtures/` before changing mobile logic.
4. Implement mobile against the spec, not against memory or UI appearance.
5. Verify there is no undocumented behavior gap between Windows and mobile.

## Hard Gate

- Do not implement a Windows-matching mobile feature from a fresh design.
- Do not collapse committed/uncommitted or pending/final states if the Windows feature keeps them separate.
- Do not replace a chunk-based pipeline with whole-buffer replacement.
- Do not leave parity assumptions implicit. If behavior differs, document it in the feature spec first.

## Repo Files To Read

- `.claude/parity/feature-template.md`
- `.claude/parity/live-translate.md`
- `.claude/CLAUDE.md`
- `parity-fixtures/`

For live translate specifically, also read the Windows source map in [references/live-translate-source-map.md](references/live-translate-source-map.md).

## Acceptance Bar

- The parity spec names the canonical Windows files and the expected state transitions.
- Shared fixtures cover the critical state machine behavior before or alongside code changes.
- Mobile code uses the same behavior model as Windows, even if the UI shell is platform-native.
- Any remaining deviation is explicit, narrow, and written down.
