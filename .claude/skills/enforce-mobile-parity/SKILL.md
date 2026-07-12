---
name: enforce-mobile-parity
description: Implement or review an Android feature that must match Windows behavior. Use for parity specs, shared fixtures, Windows WebView ports, preset/runtime parity, and any mobile change derived from an existing Windows feature.
---

# Enforce Mobile Parity

## Required Workflow

1. Read `AGENTS.md`, the matching `.claude/parity/<feature>.md`, its fixtures, and the canonical Windows source.
2. When creating a new spec, start from `.claude/parity/feature-template.md`.
3. Update the spec and shared fixtures before or with behavior changes.
4. Port the Windows state machine and contracts. Keep platform glue thin.
5. Test canonical state transitions on Windows and Android. Document every remaining deviation.

For Live Translate, read [the source map](references/live-translate-source-map.md). Use that builder/shim pattern for another Windows WebView surface only when its architecture is analogous.

## Hard Rules

- Windows behavior is canonical unless the feature spec records a narrow deviation.
- Do not fresh-design a parity feature or preserve a divergent mobile port for convenience.
- Share/extract a Windows HTML/CSS/JS surface when possible. Otherwise port its DOM and message contract with only bridge/touch shims.
- Do not collapse pending/final, committed/uncommitted, chunked/whole-buffer, or other distinct Windows states.
- Unsupported behavior must fail clearly; never pretend it works.
- Do not duplicate catalogs or retry policy. Generate/read the shared owner.
- Use current repository Material 3 APIs and shared mobile primitives; do not chase dependency versions from prose.

## Acceptance

- Spec names canonical files and state transitions.
- Fixtures cover the critical behavior.
- Mobile follows the same model, not only the same appearance.
- WebView ports preserve DOM/CSS/JS and message semantics.
- Tests cover each documented deviation and supported path.
