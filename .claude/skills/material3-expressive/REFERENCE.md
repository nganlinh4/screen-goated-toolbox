# Material 3 Expressive Reference

## Overview

Material 3 Expressive is the latest evolution of Material Design 3. Designed to complement Android 16's visual style and system UI.

## Dependencies

```toml
# gradle/libs.versions.toml
[versions]
composeBom = "2025.05.00"
composeMaterial3 = "1.5.0-alpha"

[libraries]
androidx-compose-material3 = { group = "androidx.compose.material3", name = "material3" }
```

## OptIn Annotation

Expressive components are experimental APIs — always annotate:

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun MyScreen() { ... }
```

---

## Theme Setup

### MotionScheme

Controls app-wide motion settings:

```kotlin
MaterialTheme(
    colorScheme = colorScheme,
    typography = typography,
    shapes = shapes,
    motionScheme = MotionScheme.expressive() // or standard()
) {
    // Content
}
```

### 5 Key Colors

| Color | Usage |
|-------|-------|
| Primary | Main UI elements |
| Secondary | Secondary elements |
| Tertiary | Accent color |
| Error | Error display |
| Surface | Backgrounds and cards |

### Shapes

```kotlin
val shapes = Shapes(
    extraSmall = RoundedCornerShape(4.dp),
    small = RoundedCornerShape(8.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(16.dp),
    extraLarge = RoundedCornerShape(28.dp)
)
```

### Typography

```kotlin
val typography = Typography(
    displayLarge = TextStyle(
        fontFamily = FontFamily.Default,
        fontWeight = FontWeight.Normal,
        fontSize = 57.sp,
        lineHeight = 64.sp
    ),
    // display, headline, title, body, label at each size
)
```

---

## Component Details

## Repo Patterns

### Dismiss-Zone Morphing

This repo already ships a Material 3 Expressive-inspired dismiss target system in `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/MorphDismissZone.kt`. Treat it as canonical for Android overlay dismiss interactions during the M3E revamp.

Keep these implementation details intact when editing it:

- Shared component across `BubbleService`, overlay dismissal, and help-assistant dismissal.
- Shape morph pairs:
  - primary dismiss: `Circle -> Cookie9Sided`
  - dismiss-all: `Diamond -> Clover4Leaf`
- Motion treatment:
  - overshoot entrance
  - EMA-smoothed proximity updates to avoid shaking
  - proximity-driven scale and color bloom
  - spin when the closest active target changes
  - swallow collapse after a successful drop
- Rendering treatment:
  - rotate the shape path, not the entire view, so the text label remains level
  - center the morphed path inside a larger cell to avoid clipping when scaled

When updating dismiss interactions, prefer changing `MorphDismissZone` once and reusing it instead of introducing screen-specific visual variants.

### Settings Action Morph Grammar

For small interactive surfaces in this repo, use a limited shape grammar and let morph carry the state change.

- Recommended current pairs:
  - preset runtime / model-priority action: `Square -> Cookie6Sided`
  - usage stats action: `Oval -> Gem`
  - help-assistant action: `Bun -> Flower`
  - reset action: `Slanted -> Pentagon`
  - password visibility eye toggle: `Circle -> PuffyDiamond`
- Apply the morph to the object that owns the action state:
  - action cards: usually the leading badge/object, with a subtle press-time color lift
  - utility toggles: the button container itself, keyed to the actual boolean state
- Keep icon glyphs readable and stable. If the interaction needs a lighter touch, reduce the color shift before reducing the morph.
- Avoid using many unrelated shapes in one section. Repetition is part of the expressive system.

### LoadingIndicator

Used for short wait times (under 5 seconds). Features shape-morphing animation.

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun MyLoadingScreen() {
    // Basic
    LoadingIndicator()

    // Contained
    ContainedLoadingIndicator()
}
```

### DockedToolbar

Replacement for `BottomAppBar`. Shorter and more flexible design.

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun MyDockedToolbar() {
    DockedToolbar {
        IconButton(onClick = { }) {
            Icon(Icons.Default.Home, contentDescription = "Home")
        }
        IconButton(onClick = { }) {
            Icon(Icons.Default.Search, contentDescription = "Search")
        }
    }
}
```

### FloatingToolbar

Supports both horizontal and vertical orientations. Can be combined with FAB.

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun MyFloatingToolbar() {
    FloatingToolbar(
        expanded = expanded,
        floatingActionButton = {
            FloatingActionButton(onClick = { }) {
                Icon(Icons.Default.Add, contentDescription = "Add")
            }
        }
    ) {
        // Toolbar items
    }
}
```

### FlexibleBottomAppBar

Dynamically adjusts layout and display based on scroll behavior.

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun MyFlexibleBottomAppBar() {
    FlexibleBottomAppBar(
        scrollBehavior = scrollBehavior
    ) {
        // Content
    }
}
```

### Dismiss-Zone Shape Pairing Example

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
fun singleDismiss(): List<MorphDismissZone.DismissTargetDef> = listOf(
    MorphDismissZone.DismissTargetDef(
        morph = Morph(MaterialShapes.Circle, MaterialShapes.Cookie9Sided),
        label = "×",
    ),
)
```

---

## Reference Links

- [Material 3 Official Documentation](https://developer.android.com/develop/ui/compose/designsystems/material3)
- [Material 3 Expressive Blog](https://m3.material.io/blog/building-with-m3-expressive)
- [Sample Catalog](https://github.com/meticha/material-3-expressive-catalog)
