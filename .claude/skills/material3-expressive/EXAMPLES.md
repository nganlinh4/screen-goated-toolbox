# Material 3 Expressive Implementation Examples

## Full Screen Examples

### Article List Screen

Screen example using DockedToolbar and LoadingIndicator:

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun ArticleListScreen(
    articles: List<Article>,
    isLoading: Boolean,
    onClickArticle: (Article) -> Unit
) {
    Scaffold(
        bottomBar = {
            DockedToolbar {
                IconButton(onClick = { }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_home),
                        contentDescription = null,
                    )
                }
                IconButton(onClick = { }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_search),
                        contentDescription = null,
                    )
                }
                IconButton(onClick = { }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_settings),
                        contentDescription = null,
                    )
                }
            }
        }
    ) { paddingValues ->
        if (isLoading) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(paddingValues),
                contentAlignment = Alignment.Center
            ) {
                LoadingIndicator()
            }
        } else {
            LazyColumn(
                modifier = Modifier.padding(paddingValues)
            ) {
                items(articles) { article ->
                    ArticleCard(
                        article = article,
                        onClickArticle = { onClickArticle(article) }
                    )
                }
            }
        }
    }
}
```

### Screen with FloatingToolbar

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun EditorScreen(
    content: String,
    onChangeContent: (String) -> Unit,
    onClickSave: () -> Unit
) {
    var toolbarExpanded by remember { mutableStateOf(true) }

    Scaffold(
        floatingActionButton = {
            FloatingToolbar(
                expanded = toolbarExpanded,
                floatingActionButton = {
                    FloatingActionButton(onClick = onClickSave) {
                        Icon(
                            painter = painterResource(R.drawable.ic_save),
                            contentDescription = null,
                        )
                    }
                }
            ) {
                IconButton(onClick = { /* Bold */ }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_format_bold),
                        contentDescription = null,
                    )
                }
                IconButton(onClick = { /* Italic */ }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_format_italic),
                        contentDescription = null,
                    )
                }
                IconButton(onClick = { /* Underline */ }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_format_underlined),
                        contentDescription = null,
                    )
                }
            }
        }
    ) { paddingValues ->
        TextField(
            value = content,
            onValueChange = onChangeContent,
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
        )
    }
}
```

### Scroll-responsive BottomBar

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun FeedScreen(
    items: List<FeedItem>,
    onClickItem: (FeedItem) -> Unit
) {
    val scrollBehavior = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()

    Scaffold(
        modifier = Modifier.nestedScroll(scrollBehavior.nestedScrollConnection),
        bottomBar = {
            FlexibleBottomAppBar(
                scrollBehavior = scrollBehavior
            ) {
                IconButton(onClick = { }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_home),
                        contentDescription = null,
                    )
                }
                IconButton(onClick = { }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_favorite),
                        contentDescription = null,
                    )
                }
                IconButton(onClick = { }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_person),
                        contentDescription = null,
                    )
                }
            }
        }
    ) { paddingValues ->
        LazyColumn(
            modifier = Modifier.padding(paddingValues)
        ) {
            items(items) { item ->
                FeedItemCard(
                    item = item,
                    onClickFeedItem = { onClickItem(item) }
                )
            }
        }
    }
}
```

## Standalone Component Examples

### Repo Dismiss-Zone Example

Use the shared dismiss-zone component for overlay drag dismissal instead of rebuilding a one-off M3E-like target:

```kotlin
private val dismissTargets = MorphDismissZone.singleDismiss()
private var dismissZone: MorphDismissZone? = null

private fun updateDismissZone(rawX: Float, rawY: Float) {
    val zone = dismissZone ?: MorphDismissZone(
        context = this,
        windowManager = windowManager,
        targets = dismissTargets,
    ).also { dismissZone = it; it.show() }

    zone.update(currentDismissHit(rawX, rawY).proximities)
}
```

Key point: in this repo, the dismiss bubble's morphing shape changes are part of the Material 3 Expressive revamp. Preserve them when refactoring bubble or overlay interactions.

### Repo Settings Morph Example

Use a small reusable morph helper for settings actions and stateful utility toggles instead of hand-picking new shapes every time:

```kotlin
SettingsActionButton(
    text = locale.presetRuntimeButton,
    icon = Icons.Rounded.Settings,
    morphStyle = SettingsActionMorphStyle.PRIORITY, // Square -> Cookie6Sided
    onClick = onPresetRuntimeSettingsClick,
)

MorphingVisibilityToggleButton(
    visible = isVisible,
    accent = providerAccent("Gemini", MaterialTheme.colorScheme),
    onClick = { isVisible = !isVisible },
)
```

Current repo shape grammar:

- model priority: `Square -> Cookie6Sided`
- usage stats: `Oval -> Gem`
- help assistant: `Bun -> Flower`
- reset: `Slanted -> Pentagon`
- eye toggle: `Circle -> PuffyDiamond`

### Loading State Toggle

```kotlin
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun LoadingContent(
    isLoading: Boolean,
    content: @Composable () -> Unit
) {
    if (isLoading) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            ContainedLoadingIndicator()
        }
    } else {
        content()
    }
}
```
