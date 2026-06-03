@file:OptIn(
    androidx.compose.material3.ExperimentalMaterial3Api::class,
    androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class,
)

package dev.screengoated.toolbox.mobile.translationgummy

import android.Manifest
import android.content.pm.PackageManager
import androidx.compose.animation.animateColorAsState
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.asAndroidPath
import androidx.compose.ui.graphics.asComposePath
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.material3.MaterialShapes
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

internal val CoralAccent = Color(0xFFFF7387)

@Composable
fun TranslationGummyScreen(
    locale: MobileLocaleText,
    onBack: () -> Unit,
    onNavigateToTtsSettings: () -> Unit = {},
) {
    val context = LocalContext.current
    val repository = remember(context) {
        (context.applicationContext as SgtMobileApplication).appContainer.translationGummyRepository
    }
    val state by repository.state.collectAsState()
    var autoStartAttempted by remember { mutableStateOf(false) }

    val permissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestMultiplePermissions(),
    ) { granted ->
        if (granted[Manifest.permission.RECORD_AUDIO] == true && state.appliedConfig.isValid()) {
            TranslationGummyService.start(context)
        }
    }

    fun ensureStarted(forceRestart: Boolean = false) {
        val hasPermission = ContextCompat.checkSelfPermission(
            context, Manifest.permission.RECORD_AUDIO,
        ) == PackageManager.PERMISSION_GRANTED
        if (hasPermission) {
            TranslationGummyService.start(context, restart = forceRestart)
        } else {
            permissionLauncher.launch(arrayOf(Manifest.permission.RECORD_AUDIO))
        }
    }

    val transcriptListState = rememberLazyListState()
    LaunchedEffect(
        state.transcripts.size,
        state.transcripts.lastOrNull()?.text,
        state.transcripts.lastOrNull()?.isFinal,
    ) {
        if (state.transcripts.isNotEmpty()) {
            transcriptListState.scrollToItem(state.transcripts.lastIndex)
        }
    }

    LaunchedEffect(state.appliedConfig) {
        if (!autoStartAttempted && state.appliedConfig.isValid() && !state.isRunning) {
            autoStartAttempted = true
            ensureStarted()
        }
    }

    val surfaceLow = MaterialTheme.colorScheme.surfaceContainerLow

    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ms_arrow_back), contentDescription = null)
                    }
                },
                title = {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        CompactWaveform(
                            connectionState = state.connectionState,
                            level = state.visualizerLevel,
                            accent = CoralAccent,
                            modifier = Modifier.width(60.dp).height(32.dp),
                        )
                        StatusDot(state.connectionState)
                        // Delay showing status text until Apply button is fully gone
                        var showStatusText by remember { mutableStateOf(!state.dirty) }
                        LaunchedEffect(state.dirty) {
                            if (state.dirty) {
                                showStatusText = false
                            } else {
                                kotlinx.coroutines.delay(500)
                                showStatusText = true
                            }
                        }
                        if (showStatusText) {
                            Text(
                                connectionStateLabel(state.connectionState, locale),
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                },
                actions = {
                    IconButton(
                        onClick = onNavigateToTtsSettings,
                        modifier = Modifier.testTag("translation-gummy-tts-settings"),
                    ) {
                        MorphBadge(
                            from = MaterialShapes.Cookie6Sided,
                            to = MaterialShapes.Cookie6Sided,
                            progress = 0f,
                            containerColor = CoralAccent.copy(alpha = 0.14f),
                            modifier = Modifier.size(32.dp),
                        ) {
                            Icon(
                                painterResource(R.drawable.ms_settings),
                                contentDescription = locale.voiceSettingsButton,
                                tint = CoralAccent,
                                modifier = Modifier.size(18.dp),
                            )
                        }
                    }
                    // Delay Apply appearance until status text is gone
                    var showApply by remember { mutableStateOf(false) }
                    LaunchedEffect(state.dirty) {
                        if (state.dirty) {
                            kotlinx.coroutines.delay(150)
                            showApply = true
                        } else {
                            showApply = false
                        }
                    }
                    AnimatedVisibility(
                        visible = showApply,
                        enter = fadeIn() + scaleIn(),
                        exit = fadeOut() + scaleOut(),
                    ) {
                        FilledTonalButton(
                            onClick = {
                                repository.applyDraft()
                                ensureStarted(forceRestart = true)
                            },
                            enabled = state.draftConfig.isValid(),
                            colors = ButtonDefaults.filledTonalButtonColors(
                                containerColor = lerp(surfaceLow, CoralAccent, 0.18f),
                                contentColor = CoralAccent,
                            ),
                            contentPadding = PaddingValues(horizontal = 12.dp),
                            modifier = Modifier.height(36.dp),
                        ) {
                            Text(locale.translationGummyApply, style = MaterialTheme.typography.labelMedium)
                        }
                    }
                    Spacer(Modifier.width(4.dp))
                    FilledTonalButton(
                        onClick = {
                            if (state.isRunning) {
                                TranslationGummyService.stop(context)
                            } else {
                                ensureStarted()
                            }
                        },
                        enabled = state.isRunning || state.appliedConfig.isValid(),
                        colors = ButtonDefaults.filledTonalButtonColors(
                            containerColor = if (state.isRunning) {
                                lerp(surfaceLow, MaterialTheme.colorScheme.error, 0.18f)
                            } else {
                                lerp(surfaceLow, CoralAccent, 0.18f)
                            },
                            contentColor = if (state.isRunning) {
                                MaterialTheme.colorScheme.error
                            } else {
                                CoralAccent
                            },
                        ),
                        contentPadding = PaddingValues(horizontal = 12.dp),
                        modifier = Modifier.height(36.dp).padding(end = 10.dp),
                    ) {
                        Icon(
                            painterResource(if (state.isRunning) R.drawable.ms_stop else R.drawable.ms_play_arrow),
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(4.dp))
                        Text(
                            if (state.isRunning) locale.translationGummyStop else locale.translationGummyStart,
                            style = MaterialTheme.typography.labelMedium,
                        )
                    }
                },
            )
        },
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .padding(horizontal = 14.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            // Language Card 1
            TranslationGummyLanguageCard(
                number = "1",
                title = locale.translationGummyFirstProfile,
                profile = state.draftConfig.first,
                languagePlaceholder = locale.translationGummyLanguageLabel,
                accentPlaceholder = locale.translationGummyAccentLabel,
                tonePlaceholder = locale.translationGummyToneLabel,
                accent = CoralAccent,
                onChanged = { repository.updateDraft { it.copy(first = it.first.copy(
                    language = if (it.first.language != state.draftConfig.first.language) it.first.language else state.draftConfig.first.language,
                    accent = if (it.first.accent != state.draftConfig.first.accent) it.first.accent else state.draftConfig.first.accent,
                    tone = if (it.first.tone != state.draftConfig.first.tone) it.first.tone else state.draftConfig.first.tone,
                )) } },
                onLanguageChanged = { repository.updateDraft { c -> c.copy(first = c.first.copy(language = it)) } },
                onAccentChanged = { repository.updateDraft { c -> c.copy(first = c.first.copy(accent = it)) } },
                onToneChanged = { repository.updateDraft { c -> c.copy(first = c.first.copy(tone = it)) } },
            )

            // Language Card 2
            TranslationGummyLanguageCard(
                number = "2",
                title = locale.translationGummySecondProfile,
                profile = state.draftConfig.second,
                languagePlaceholder = locale.translationGummyLanguageLabel,
                accentPlaceholder = locale.translationGummyAccentLabel,
                tonePlaceholder = locale.translationGummyToneLabel,
                accent = CoralAccent,
                onLanguageChanged = { repository.updateDraft { c -> c.copy(second = c.second.copy(language = it)) } },
                onAccentChanged = { repository.updateDraft { c -> c.copy(second = c.second.copy(accent = it)) } },
                onToneChanged = { repository.updateDraft { c -> c.copy(second = c.second.copy(tone = it)) } },
            )

            // Error
            state.lastError?.takeIf(String::isNotBlank)?.let { error ->
                Text(
                    text = error,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(horizontal = 4.dp),
                )
            }

            // Transcript
            TranscriptCard(
                transcripts = state.transcripts,
                emptyLabel = locale.translationGummyNoTranscriptYet,
                inputChip = locale.translationGummyInputChip,
                outputChip = locale.translationGummyOutputChip,
                accent = CoralAccent,
                listState = transcriptListState,
                modifier = Modifier.weight(1f),
            )
        }
    }

    if (!state.guideSeen) {
        TranslationGummyGuideDialog(
            title = locale.translationGummyTitle,
            message = locale.translationGummyGuide,
            confirmLabel = locale.translationGummyGuideOk,
            onDismiss = repository::dismissGuide,
        )
    }
}

// ── Language Card ──


internal fun connectionStateLabel(
    state: TranslationGummyConnectionState,
    locale: MobileLocaleText,
): String = when (state) {
    TranslationGummyConnectionState.NOT_CONFIGURED -> locale.translationGummyStatusNotConfigured
    TranslationGummyConnectionState.CONNECTING -> locale.translationGummyStatusConnecting
    TranslationGummyConnectionState.READY -> locale.translationGummyStatusReady
    TranslationGummyConnectionState.RECONNECTING -> locale.translationGummyStatusReconnecting
    TranslationGummyConnectionState.ERROR -> locale.translationGummyStatusError
    TranslationGummyConnectionState.STOPPED -> locale.translationGummyStatusStopped
}
