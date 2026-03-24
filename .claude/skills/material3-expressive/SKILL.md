---
name: material3-expressive
description: Skill for implementing UI following Material 3 Expressive guidelines. Use when creating screens or components with Jetpack Compose. Provides MotionScheme, new Expressive components, and theming patterns.
---

# Material 3 Expressive UI Creation Guide

When creating UI with Jetpack Compose, follow **Material 3 Expressive** guidelines.

For this repo, treat the Android dismiss-zone revamp as part of the Material 3 Expressive work, not as a separate overlay hack. When touching bubble dismissal, preset overlay dismissal, or help-assistant dismissal, preserve the shared morphing target behavior implemented in `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/MorphDismissZone.kt`.

## Requirements

| Requirement | Value |
|-------------|-------|
| minSdk | 23 or higher |
| Material3 | 1.5.0-alpha or higher (includes Expressive components) |
| OptIn | `@OptIn(ExperimentalMaterial3ExpressiveApi::class)` |

## Quick Reference

### Theme Setup
```kotlin
MaterialTheme(
    colorScheme = colorScheme,
    typography = typography,
    shapes = shapes,
    motionScheme = MotionScheme.expressive()
) { content() }
```

### Recommended Components

| Use Case | Component | Notes |
|----------|-----------|-------|
| Loading indicator | `LoadingIndicator` | For wait times under 5 seconds |
| Loading indicator (contained) | `ContainedLoadingIndicator` | - |
| Bottom toolbar | `DockedToolbar` | Replacement for BottomAppBar |
| Floating toolbar | `FloatingToolbar` | Supports horizontal & vertical |
| Flexible bottom bar | `FlexibleBottomAppBar` | Scroll-responsive |

### Deprecated -> Replacement

| Deprecated | Replacement |
|------------|-------------|
| `BottomAppBar` | `DockedToolbar` |
| `CircularProgressIndicator` (short waits) | `LoadingIndicator` |

## Best Practices

1. Use `MotionScheme.expressive()` for fluid animations
2. Leverage shape morphing
3. Follow color roles (automatic accessibility compliance)
4. Support dynamic color on Android 12+
5. Express elevation through tonal color overlays

## Repo-Specific Contract

- Do not replace the dismiss-zone shapes with plain circles, chips, or static icons when working on the Android M3E revamp.
- Reuse `MorphDismissZone` for bubble, preset overlay, and help-assistant dismiss targets instead of forking custom implementations.
- Preserve the current morph pairs unless product direction changes:
  - single dismiss: `MaterialShapes.Circle -> MaterialShapes.Cookie9Sided`
  - dismiss-all: `MaterialShapes.Diamond -> MaterialShapes.Clover4Leaf`
- Preserve the expressive motion treatment that already shipped with the revamp:
  - overshoot animate-in
  - proximity-driven morph/scale/color feedback
  - target-change spin
  - swallow animation on successful dismiss
- Keep the label upright while the shape rotates; in this repo the path rotates independently from the text.
- Keep the larger layout cell around the shape so bloom/scale-up animation does not clip.
- For mobile settings actions and small utility toggles, prefer a constrained morph grammar instead of picking arbitrary shapes from the whole catalog.
- Current repo-approved settings/action morph pairs:
  - preset runtime / "Uu tien model": `Square -> Cookie6Sided`
  - usage stats / "Thong ke model": `Oval -> Gem`
  - help assistant / "Hoi cach dung": `Bun -> Flower`
  - reset / destructive utility: `Slanted -> Pentagon`
  - password visibility toggle eye button: `Circle -> PuffyDiamond`
- For stateful utility controls, shape morph is the primary signal and color shift is secondary.
- Reuse a small family of recurring shapes across a section; do not turn one row into a random catalog of unrelated shapes.

## Details

- Component details & theming: `REFERENCE.md`
- Implementation examples: `EXAMPLES.md`
