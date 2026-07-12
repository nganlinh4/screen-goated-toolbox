---
name: material3-expressive
description: Implement or review Android Jetpack Compose UI in this repository using its Material 3 Expressive theme, shared morph components, and established interaction grammar. Use for Android screens, settings controls, floating overlays, and dismiss targets.
---

# Material 3 Expressive

## Workflow

1. Read `mobile/gradle/libs.versions.toml` and the existing component. Never copy dependency versions from prose.
2. Reuse repository theme and primitives before adding a new visual system.
3. Use `@OptIn(ExperimentalMaterial3ExpressiveApi::class)` only where the selected API requires it.
4. Verify motion, touch targets, clipping, dynamic color, light/dark themes, and accessibility.

## Canonical Repository Patterns

- Theme and settings primitives: `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/`
- Shared overlay dismiss target: `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/MorphDismissZone.kt`
- Keep dismiss morphs centralized. Do not fork screen-specific copies.
- Rotate the morph path, not its label. Leave enough layout space for bloom/scale motion.
- Preserve the current dismiss pairs unless product direction changes:
  - single: `Circle -> Cookie9Sided`
  - dismiss-all: `Diamond -> Clover4Leaf`
- Reuse `ExpressiveMorphPair` and `ExpressiveSettingsMorphStyle` for settings actions. Treat their source definitions as canonical; do not duplicate the full catalog here.
- Shape is the primary state cue; color is secondary. Use a small recurring shape family, not random per-row shapes.

## Guardrails

- Material 3 Expressive is not permission to redesign a Windows-parity surface. Follow `.claude/skills/enforce-mobile-parity/SKILL.md` when parity applies.
- Do not replace working semantic components with a fashionable API solely because it is newer.
- Keep shared motion/state in one owner; Compose callers should be thin.
