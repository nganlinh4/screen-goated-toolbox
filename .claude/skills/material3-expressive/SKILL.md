---
name: material3-expressive
description: Skill for implementing UI following Material 3 Expressive guidelines. Use when creating screens or components with Jetpack Compose. Provides MotionScheme, new Expressive components, and theming patterns.
---

# Material 3 Expressive UI Creation Guide

When creating UI with Jetpack Compose, follow **Material 3 Expressive** guidelines.

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

## Details

- Component details & theming: `REFERENCE.md`
- Implementation examples: `EXAMPLES.md`
