@file:OptIn(
    ExperimentalMaterial3Api::class,
    ExperimentalMaterial3ExpressiveApi::class,
    ExperimentalSharedTransitionApi::class,
)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ExperimentalSharedTransitionApi
import androidx.compose.animation.SharedTransitionLayout
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.toPath
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.graphics.shapes.CornerRounding
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon

import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.downloader.DownloaderViewModel
import dev.screengoated.toolbox.mobile.downloader.ui.DownloaderScreen
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
fun SgtMobileApp(
    state: LiveSessionState,
    apiKey: String,
    cerebrasApiKey: String,
    groqApiKey: String,
    openRouterApiKey: String,
    ollamaUrl: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    presetRuntimeSettings: PresetRuntimeSettings,
    uiPreferences: MobileUiPreferences,
    locale: MobileLocaleText,
    edgeVoiceCatalogState: EdgeVoiceCatalogState,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onGroqApiKeyChanged: (String) -> Unit,
    onOpenRouterApiKeyChanged: (String) -> Unit,
    onOllamaUrlChanged: (String) -> Unit,
    onPresetRuntimeSettingsChanged: (PresetRuntimeSettings) -> Unit,
    onUiLanguageSelected: (String) -> Unit,
    onThemeCycleRequested: () -> Unit,
    onGlobalTtsMethodChanged: (MobileTtsMethod) -> Unit,
    onGlobalTtsSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onGlobalTtsVoiceChanged: (String) -> Unit,
    onGlobalTtsConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onGlobalEdgeTtsSettingsChanged: (MobileEdgeTtsSettings) -> Unit,
    onVoiceSettingsShown: () -> Unit,
    onRetryEdgeVoiceCatalog: () -> Unit,
    onPreviewGeminiVoice: (String) -> Unit,
    onPreviewEdgeVoice: (String, String) -> Unit,
    onSessionToggle: () -> Unit,
) {
    var showTtsSettings by rememberSaveable { mutableStateOf(false) }
    var showPresetRuntimeSettings by rememberSaveable { mutableStateOf(false) }
    var showDownloader by rememberSaveable { mutableStateOf(false) }
    var showDj by rememberSaveable { mutableStateOf(false) }
    var activePresetId by rememberSaveable { mutableStateOf<String?>(null) }
    val presetRepository = (LocalContext.current.applicationContext as SgtMobileApplication)
        .appContainer
        .presetRepository
    val presetCatalog by presetRepository.catalogState.collectAsState()

    if (showTtsSettings) {
        GlobalTtsSettingsDialog(
            settings = globalTtsSettings,
            locale = locale,
            edgeVoiceCatalogState = edgeVoiceCatalogState,
            onDismiss = { showTtsSettings = false },
            onMethodChanged = onGlobalTtsMethodChanged,
            onSpeedPresetChanged = onGlobalTtsSpeedPresetChanged,
            onVoiceChanged = onGlobalTtsVoiceChanged,
            onConditionsChanged = onGlobalTtsConditionsChanged,
            onEdgeSettingsChanged = onGlobalEdgeTtsSettingsChanged,
            onRetryEdgeVoiceCatalog = onRetryEdgeVoiceCatalog,
            onPreviewGeminiVoice = onPreviewGeminiVoice,
            onPreviewEdgeVoice = onPreviewEdgeVoice,
        )
    }

    if (showPresetRuntimeSettings) {
        PresetRuntimeSettingsDialog(
            settings = presetRuntimeSettings,
            locale = locale,
            onDismiss = { showPresetRuntimeSettings = false },
            onSave = {
                onPresetRuntimeSettingsChanged(it)
                showPresetRuntimeSettings = false
            },
        )
    }

    val focusManager = LocalFocusManager.current
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.surface)
            .pointerInput(Unit) { detectTapGestures(onTap = { focusManager.clearFocus() }) },
    ) {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                if (!isLandscape) {
                    CenterAlignedTopAppBar(
                        colors = TopAppBarDefaults.topAppBarColors(
                            containerColor = Color.Transparent,
                        ),
                        title = {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(10.dp),
                            ) {
                                SgtBrandBadge(
                                    size = 28.dp,
                                    showBackground = false,
                                )
                                Text(
                                    text = locale.appHeaderTitle,
                                    style = MaterialTheme.typography.titleMedium,
                                )
                            }
                        },
                        navigationIcon = {
                            LanguageMorphToggle(
                                uiLanguage = uiPreferences.uiLanguage,
                                languageOptions = locale.languageOptions,
                                onLanguageSelected = onUiLanguageSelected,
                            )
                        },
                        actions = {
                            ThemeMorphToggle(
                                themeMode = uiPreferences.themeMode,
                                onClick = onThemeCycleRequested,
                                contentDescription = "${locale.themeCycleLabel}: ${locale.themeModeLabels[uiPreferences.themeMode]}",
                            )
                        },
                    )
                }
            },
        ) { innerPadding ->
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(innerPadding),
            ) {
                MobileShellSurface(
                    state = state,
                    apiKey = apiKey,
                    cerebrasApiKey = cerebrasApiKey,
                    groqApiKey = groqApiKey,
                    openRouterApiKey = openRouterApiKey,
                    ollamaUrl = ollamaUrl,
                    globalTtsSettings = globalTtsSettings,
                    presetRuntimeSettings = presetRuntimeSettings,
                    locale = locale,
                    onApiKeyChanged = onApiKeyChanged,
                    onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                    onGroqApiKeyChanged = onGroqApiKeyChanged,
                    onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                    onOllamaUrlChanged = onOllamaUrlChanged,
                    onPresetRuntimeSettingsClick = { showPresetRuntimeSettings = true },
                    onVoiceSettingsClick = {
                        onVoiceSettingsShown()
                        showTtsSettings = true
                    },
                    showEmbeddedHeader = isLandscape,
                    appHeaderTitle = locale.appHeaderTitle,
                    uiLanguage = uiPreferences.uiLanguage,
                    languageOptions = locale.languageOptions,
                    onUiLanguageSelected = onUiLanguageSelected,
                    themeMode = uiPreferences.themeMode,
                    onThemeCycleRequested = onThemeCycleRequested,
                    onSessionToggle = onSessionToggle,
                    onDownloaderClick = { showDownloader = true },
                    onDjClick = { showDj = true },
                    onPresetClick = { presetId -> activePresetId = presetId },
                )
            }
        }

        // Downloader overlay with container-transform-style animation
        if (showDownloader) {
            androidx.activity.compose.BackHandler { showDownloader = false }
        }
        androidx.compose.animation.AnimatedVisibility(
            visible = showDownloader,
            enter = fadeIn(tween(200)) + androidx.compose.animation.scaleIn(
                initialScale = 0.8f,
                animationSpec = tween(350, easing = androidx.compose.animation.core.FastOutSlowInEasing),
            ),
            exit = fadeOut(tween(150)) + androidx.compose.animation.scaleOut(
                targetScale = 0.8f,
                animationSpec = tween(250, easing = androidx.compose.animation.core.FastOutSlowInEasing),
            ),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(MaterialTheme.colorScheme.surface),
            ) {
                DownloaderScreenWrapper(locale = locale, onBack = { showDownloader = false })
            }
        }

        // DJ overlay
        if (showDj) {
            androidx.activity.compose.BackHandler { showDj = false }
        }
        androidx.compose.animation.AnimatedVisibility(
            visible = showDj,
            enter = fadeIn(tween(200)) + androidx.compose.animation.scaleIn(
                initialScale = 0.8f,
                animationSpec = tween(350, easing = androidx.compose.animation.core.FastOutSlowInEasing),
            ),
            exit = fadeOut(tween(150)) + androidx.compose.animation.scaleOut(
                targetScale = 0.8f,
                animationSpec = tween(250, easing = androidx.compose.animation.core.FastOutSlowInEasing),
            ),
        ) {
            val isDjDark = when (uiPreferences.themeMode) {
                MobileThemeMode.SYSTEM -> androidx.compose.foundation.isSystemInDarkTheme()
                MobileThemeMode.DARK -> true
                MobileThemeMode.LIGHT -> false
            }
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(MaterialTheme.colorScheme.surface),
            ) {
                DjScreen(
                    apiKey = apiKey,
                    isDark = isDjDark,
                    lang = uiPreferences.uiLanguage,
                    onBack = { showDj = false },
                )
            }
        }

        // Preset editor — opens directly (no inspector intermediary)
        val activePreset = activePresetId?.let { id -> presetCatalog.findPreset(id) }
        if (activePreset != null) {
            val presetLang = uiPreferences.uiLanguage
            androidx.activity.compose.BackHandler { activePresetId = null }
            androidx.compose.animation.AnimatedVisibility(
                visible = true,
                enter = fadeIn(tween(200)) + androidx.compose.animation.scaleIn(
                    initialScale = 0.9f,
                    animationSpec = tween(300, easing = androidx.compose.animation.core.FastOutSlowInEasing),
                ),
            ) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(MaterialTheme.colorScheme.surface),
                ) {
                    dev.screengoated.toolbox.mobile.preset.ui.PresetEditorScreen(
                        preset = activePreset.preset,
                        lang = presetLang,
                        onBack = { activePresetId = null },
                        onPresetChanged = { updated ->
                            presetRepository.updateBuiltInOverride(activePreset.preset.id) { updated }
                        },
                        onFavoriteToggle = {
                            presetRepository.toggleFavorite(activePreset.preset.id)
                        },
                        onRestoreDefault = {
                            presetRepository.restoreBuiltInPreset(activePreset.preset.id)
                            activePresetId = null
                        },
                    )
                }
            }
        }
    }
}

// Custom A shape: triangle silhouette with a V-notch at the bottom implying the legs + crossbar.
private val AShape = RoundedPolygon(
    vertices = floatArrayOf(
         0.00f, -1.00f,   // top tip
         0.85f,  1.00f,   // bottom-right foot
         0.22f,  0.05f,   // inner-right (concave — creates leg gap)
         0.00f,  0.35f,   // notch bottom (V point)
        -0.22f,  0.05f,   // inner-left (concave — creates leg gap)
        -0.85f,  1.00f,   // bottom-left foot
    ),
    perVertexRounding = listOf(
        CornerRounding(0.05f),  // top tip — barely rounded
        CornerRounding(0.25f),  // bottom-right foot
        CornerRounding(0.40f),  // inner-right concave
        CornerRounding(0.55f),  // notch bottom — smooth V
        CornerRounding(0.40f),  // inner-left concave
        CornerRounding(0.25f),  // bottom-left foot
    ),
)

private val ThemeShapes = arrayOf(
    AShape,                  // System/Auto — stylised A
    MaterialShapes.SemiCircle, // Dark        — moon
    MaterialShapes.Sunny,      // Light       — radiating sun
)

private val ThemeColors = arrayOf(
    Color(0xFF8AB4F8),  // Auto — blue
    Color(0xFFD0BCFF),  // Dark — purple
    Color(0xFFFFCC80),  // Light — amber
)

private val ThemeRotations = floatArrayOf(
    0f,     // Auto
    120f,   // Dark
    240f,   // Light
)

@Composable
internal fun ThemeMorphToggle(
    themeMode: MobileThemeMode,
    onClick: () -> Unit,
    contentDescription: String,
) {
    val idx = themeMode.ordinal.coerceIn(0, 2)

    val morphProgress by animateFloatAsState(
        targetValue = idx.toFloat(),
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessLow,
        ),
        label = "morph-progress",
    )
    val rotation by animateFloatAsState(
        targetValue = ThemeRotations[idx],
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessLow,
        ),
        label = "morph-rotation",
    )

    val fromIdx = morphProgress.toInt().coerceIn(0, 1)
    val toIdx = (fromIdx + 1).coerceIn(0, 2)
    val segmentT = (morphProgress - fromIdx).coerceIn(0f, 1f)

    val morph = remember(fromIdx, toIdx) {
        Morph(ThemeShapes[fromIdx], ThemeShapes[toIdx])
    }

    val color = lerpColor(ThemeColors[fromIdx], ThemeColors[toIdx], segmentT)

    IconButton(onClick = onClick) {
        Canvas(modifier = Modifier.size(28.dp)) {
            val path = morph.toPath(progress = segmentT)
            val s = size.minDimension
            // Compute actual path bounds to find visual centroid and natural size.
            val bounds = path.getBounds()
            val pathSize = maxOf(bounds.width, bounds.height).takeIf { it > 0f } ?: 1f
            val cx = bounds.left + bounds.width / 2f
            val cy = bounds.top + bounds.height / 2f
            // Scale to fill ~90% of the canvas.
            val scale = s * 0.90f / pathSize
            val matrix = Matrix()
            // Point execution order: center shape → rotate → scale → move to canvas center
            matrix.translate(s / 2f, s / 2f)
            matrix.scale(scale, scale)
            matrix.rotateZ(rotation)
            matrix.translate(-cx, -cy)
            path.transform(matrix)
            drawPath(path, color = color)
        }
    }
}

// ── Language morph toggle (E / V / K) ────────────────────────────────────────

// E — three horizontal bars from a left spine
private val EShape = RoundedPolygon(
    vertices = floatArrayOf(
        -1.00f, -1.00f,  //  0 TL
         1.00f, -1.00f,  //  1 top-bar right
         1.00f, -0.45f,  //  2 top-bar inner-right
        -0.10f, -0.45f,  //  3 spine junction top
        -0.10f, -0.08f,  //  4 mid-bar top
         0.70f, -0.08f,  //  5 mid-bar right
         0.70f,  0.08f,  //  6 mid-bar inner-right
        -0.10f,  0.08f,  //  7 spine junction mid
        -0.10f,  0.45f,  //  8 spine junction bot
         1.00f,  0.45f,  //  9 bot-bar inner-right
         1.00f,  1.00f,  // 10 BR
        -1.00f,  1.00f,  // 11 BL
    ),
    perVertexRounding = listOf(
        CornerRounding(0.15f), CornerRounding(0.15f), CornerRounding(0.15f),
        CornerRounding(0.08f), CornerRounding(0.08f), CornerRounding(0.15f),
        CornerRounding(0.15f), CornerRounding(0.08f), CornerRounding(0.08f),
        CornerRounding(0.15f), CornerRounding(0.15f), CornerRounding(0.15f),
    ),
)

// V — downward chevron with a sharp tip
private val VShape = RoundedPolygon(
    vertices = floatArrayOf(
        -0.90f, -1.00f,  // top-left outer
         0.00f,  1.00f,  // bottom tip
         0.90f, -1.00f,  // top-right outer
         0.50f, -1.00f,  // top-right inner
         0.00f,  0.18f,  // inner V apex
        -0.50f, -1.00f,  // top-left inner
    ),
    perVertexRounding = listOf(
        CornerRounding(0.20f),  // top-left outer
        CornerRounding(0.04f),  // bottom tip — sharp
        CornerRounding(0.20f),  // top-right outer
        CornerRounding(0.20f),  // top-right inner
        CornerRounding(0.45f),  // inner apex — smooth
        CornerRounding(0.20f),  // top-left inner
    ),
)

// K — left spine + two diagonal arms meeting at a concave junction
private val KShape = RoundedPolygon(
    vertices = floatArrayOf(
        -1.00f, -1.00f,  // TL
        -0.25f, -1.00f,  // top of spine
         0.90f, -1.00f,  // top of upper arm (outer)
         0.05f,  0.00f,  // concave junction (arms meet spine)
         0.90f,  1.00f,  // bottom of lower arm (outer)
        -0.25f,  1.00f,  // bottom of spine
        -1.00f,  1.00f,  // BL
    ),
    perVertexRounding = listOf(
        CornerRounding(0.15f),  // TL
        CornerRounding(0.10f),  // top of spine
        CornerRounding(0.20f),  // top arm tip
        CornerRounding(0.05f),  // concave junction — sharp to keep the K notch
        CornerRounding(0.20f),  // bottom arm tip
        CornerRounding(0.10f),  // bottom of spine
        CornerRounding(0.15f),  // BL
    ),
)

private val LanguageShapes = arrayOf(EShape, VShape, KShape)

private val LanguageColors = arrayOf(
    Color(0xFF82B1FF),  // E — soft blue
    Color(0xFFFF8A80),  // V — coral
    Color(0xFF69F0AE),  // K — mint
)

private val LanguageRotations = floatArrayOf(0f, 0f, 0f)

@Composable
internal fun LanguageMorphToggle(
    uiLanguage: String,
    languageOptions: List<dev.screengoated.toolbox.mobile.ui.i18n.MobileUiLanguageOption>,
    onLanguageSelected: (String) -> Unit,
) {
    val idx = languageOptions.indexOfFirst { it.code == uiLanguage }.coerceAtLeast(0)

    val morphProgress by animateFloatAsState(
        targetValue = idx.toFloat(),
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessLow,
        ),
        label = "lang-morph",
    )
    val rotation by animateFloatAsState(
        targetValue = LanguageRotations[idx.coerceIn(0, 2)],
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessLow,
        ),
        label = "lang-rotation",
    )

    val fromIdx = morphProgress.toInt().coerceIn(0, LanguageShapes.size - 2)
    val toIdx = (fromIdx + 1).coerceIn(0, LanguageShapes.size - 1)
    val segmentT = (morphProgress - fromIdx).coerceIn(0f, 1f)
    val morph = remember(fromIdx, toIdx) { Morph(LanguageShapes[fromIdx], LanguageShapes[toIdx]) }
    val color = lerpColor(LanguageColors[fromIdx], LanguageColors[toIdx], segmentT)

    IconButton(
        onClick = {
            val next = languageOptions[(idx + 1) % languageOptions.size].code
            onLanguageSelected(next)
        },
    ) {
        Canvas(modifier = Modifier.size(28.dp)) {
            val path = morph.toPath(progress = segmentT)
            val s = size.minDimension
            val bounds = path.getBounds()
            val pathSize = maxOf(bounds.width, bounds.height).takeIf { it > 0f } ?: 1f
            val cx = bounds.left + bounds.width / 2f
            val cy = bounds.top + bounds.height / 2f
            val scale = s * 0.90f / pathSize
            val matrix = Matrix()
            matrix.translate(s / 2f, s / 2f)
            matrix.scale(scale, scale)
            matrix.rotateZ(rotation)
            matrix.translate(-cx, -cy)
            path.transform(matrix)
            drawPath(path, color = color)
        }
    }
}

private fun lerpColor(a: Color, b: Color, t: Float): Color = Color(
    red = a.red + (b.red - a.red) * t,
    green = a.green + (b.green - a.green) * t,
    blue = a.blue + (b.blue - a.blue) * t,
    alpha = 1f,
)

@Composable
private fun DownloaderScreenWrapper(locale: MobileLocaleText, onBack: () -> Unit) {
    val context = LocalContext.current
    val app = context.applicationContext as SgtMobileApplication
    val vm: DownloaderViewModel = viewModel(
        factory = DownloaderViewModel.factory(app.appContainer.downloaderRepository),
    )
    DownloaderScreen(viewModel = vm, locale = locale, onBack = onBack)
}
