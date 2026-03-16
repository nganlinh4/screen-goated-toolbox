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
2. If the Windows feature uses HTML/CSS/JS/WebView, inspect that web surface before writing any mobile UI code.
3. Update or create the matching spec under `.claude/parity/`.
4. Add or update shared fixtures under `parity-fixtures/` before changing mobile logic.
5. Implement mobile against the spec and canonical source files, not against memory or UI appearance.
6. Verify there is no undocumented behavior gap between Windows and mobile.

## Hard Gate

- Do not implement a Windows-matching mobile feature from a fresh design.
- Do not hand-rebuild a Windows WebView overlay in custom Android HTML/CSS/JS or Compose when the Windows web surface can be shared, extracted, or ported verbatim.
- For Windows WebView overlays, use the Live Translate pattern:
  - Windows HTML/CSS/JS remains the source of truth
  - Android assembles that surface through a builder/template layer
  - Android-specific code is limited to bridge glue, mobile-only interaction shims, and explicitly documented unsupported controls
- Do not collapse committed/uncommitted or pending/final states if the Windows feature keeps them separate.
- Do not replace a chunk-based pipeline with whole-buffer replacement.
- Do not leave parity assumptions implicit. If behavior differs, document it in the feature spec first.

## Repo Files To Read

- `.claude/parity/feature-template.md`
- `.claude/parity/live-translate.md`
- `.claude/CLAUDE.md`
- `parity-fixtures/`

For live translate specifically, also read the Windows source map in [references/live-translate-source-map.md](references/live-translate-source-map.md).
For any Windows HTML/WebView overlay, identify whether the mobile path should be:
- extracted to shared assets/templates, or
- verbatim-ported with a thin Android bridge if extraction does not exist yet.

## Acceptance Bar

- The parity spec names the canonical Windows files and the expected state transitions.
- Shared fixtures cover the critical state machine behavior before or alongside code changes.
- Mobile code uses the same behavior model as Windows, even if the UI shell is platform-native.
- For Windows WebView overlays, mobile uses the same DOM/CSS/JS contract or a documented extraction of it, not an independently restyled clone.
- Any remaining deviation is explicit, narrow, and written down.
