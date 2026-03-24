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

---

## Reference Links

- [Material 3 Official Documentation](https://developer.android.com/develop/ui/compose/designsystems/material3)
- [Material 3 Expressive Blog](https://m3.material.io/blog/building-with-m3-expressive)
- [Sample Catalog](https://github.com/meticha/material-3-expressive-catalog)
