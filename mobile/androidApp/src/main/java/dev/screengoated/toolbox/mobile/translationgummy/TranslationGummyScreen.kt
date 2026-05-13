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

private val CoralAccent = Color(0xFFFF7387)

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

@Composable
private fun TranslationGummyLanguageCard(
    number: String,
    title: String,
    profile: TranslationGummyLanguageProfile,
    languagePlaceholder: String,
    accentPlaceholder: String,
    tonePlaceholder: String,
    accent: Color,
    onChanged: ((TranslationGummyLanguageProfile) -> Unit)? = null,
    onLanguageChanged: (String) -> Unit,
    onAccentChanged: (String) -> Unit,
    onToneChanged: (String) -> Unit,
) {
    val surfaceLow = MaterialTheme.colorScheme.surfaceContainerLow
    Card(
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = lerp(surfaceLow, accent, 0.05f),
        ),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            // Row 1: Number badge + title + language input
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                // Number badge — soft Bun shape
                MorphBadge(
                    from = MaterialShapes.SoftBoom,
                    to = MaterialShapes.SoftBoom,
                    progress = 0f,
                    containerColor = accent.copy(alpha = 0.15f),
                    modifier = Modifier.size(28.dp),
                ) {
                    Text(
                        number,
                        style = MaterialTheme.typography.labelMedium,
                        fontWeight = FontWeight.Bold,
                        color = accent,
                    )
                }
                Text(
                    title,
                    style = MaterialTheme.typography.titleSmall,
                    color = accent,
                    fontWeight = FontWeight.SemiBold,
                )
                OutlinedTextField(
                    value = profile.language,
                    onValueChange = onLanguageChanged,
                    placeholder = { Text(languagePlaceholder, style = MaterialTheme.typography.bodySmall) },
                    modifier = Modifier.weight(1f).defaultMinSize(minHeight = 44.dp),
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodyMedium,
                    shape = MaterialTheme.shapes.large,
                    colors = translationGummyTextFieldColors(accent),
                )
            }
            // Row 2: Accent + Tone
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                OutlinedTextField(
                    value = profile.accent,
                    onValueChange = onAccentChanged,
                    placeholder = { Text(accentPlaceholder, style = MaterialTheme.typography.bodySmall) },
                    modifier = Modifier.weight(1f).defaultMinSize(minHeight = 44.dp),
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodySmall,
                    shape = MaterialTheme.shapes.large,
                    colors = translationGummyTextFieldColors(accent),
                )
                OutlinedTextField(
                    value = profile.tone,
                    onValueChange = onToneChanged,
                    placeholder = { Text(tonePlaceholder, style = MaterialTheme.typography.bodySmall) },
                    modifier = Modifier.weight(1f).defaultMinSize(minHeight = 44.dp),
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodySmall,
                    shape = MaterialTheme.shapes.large,
                    colors = translationGummyTextFieldColors(accent),
                )
            }
        }
    }
}

@Composable
private fun translationGummyTextFieldColors(accent: Color): TextFieldColors {
    return OutlinedTextFieldDefaults.colors(
        focusedContainerColor = accent.copy(alpha = 0.08f),
        unfocusedContainerColor = accent.copy(alpha = 0.04f),
        focusedBorderColor = accent.copy(alpha = 0.6f),
        unfocusedBorderColor = accent.copy(alpha = 0.2f),
        cursorColor = accent,
    )
}

// ── Transcript ──

private sealed class TranscriptEntry {
    data class Pair(
        val id: Long,
        val input: String,
        val output: String,
        val lang: String,
        val placement: TranscriptBubblePlacement,
    ) : TranscriptEntry()
    data class Sep(val id: Long, val time: String) : TranscriptEntry()
}

private enum class TranscriptBubblePlacement {
    CENTER,
    LEFT,
    RIGHT,
}

private fun groupEntries(items: List<TranslationGummyTranscriptItem>): List<TranscriptEntry> {
    val entries = mutableListOf<TranscriptEntry>()
    var i = 0
    while (i < items.size) {
        val item = items[i]
        if (item.role == TranslationGummyTranscriptRole.SEPARATOR) {
            entries += TranscriptEntry.Sep(item.id, item.text)
            i++
            continue
        }
        if (item.role == TranslationGummyTranscriptRole.INPUT) {
            val next = items.getOrNull(i + 1)
            if (next != null && next.role == TranslationGummyTranscriptRole.OUTPUT) {
                entries += TranscriptEntry.Pair(item.id, item.text, next.text, next.lang, TranscriptBubblePlacement.CENTER)
                i += 2
            } else {
                entries += TranscriptEntry.Pair(item.id, item.text, "", "", TranscriptBubblePlacement.CENTER)
                i++
            }
        } else {
            entries += TranscriptEntry.Pair(item.id, "", item.text, item.lang, TranscriptBubblePlacement.CENTER)
            i++
        }
    }
    // Determine alignment by first detected lang
    val firstLang = entries.filterIsInstance<TranscriptEntry.Pair>().firstOrNull { it.lang.isNotBlank() }?.lang.orEmpty()
    if (firstLang.isNotBlank()) {
        for (idx in entries.indices) {
            val e = entries[idx]
            if (e is TranscriptEntry.Pair) {
                val placement = when {
                    e.lang.isBlank() -> TranscriptBubblePlacement.CENTER
                    e.lang == firstLang -> TranscriptBubblePlacement.LEFT
                    else -> TranscriptBubblePlacement.RIGHT
                }
                entries[idx] = e.copy(placement = placement)
            }
        }
    }
    return entries
}

@Composable
private fun TranscriptCard(
    transcripts: List<TranslationGummyTranscriptItem>,
    emptyLabel: String,
    inputChip: String,
    outputChip: String,
    accent: Color,
    listState: androidx.compose.foundation.lazy.LazyListState,
    modifier: Modifier = Modifier,
) {
    val surfaceLow = MaterialTheme.colorScheme.surfaceContainerLow
    Card(
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = lerp(surfaceLow, accent, 0.03f),
        ),
        modifier = modifier,
    ) {
        if (transcripts.isEmpty()) {
            Box(
                modifier = Modifier.fillMaxSize().padding(24.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    emptyLabel,
                    style = MaterialTheme.typography.bodyMedium,
                    fontStyle = FontStyle.Italic,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                )
            }
        } else {
            val entries = remember(transcripts) { groupEntries(transcripts) }
            LazyColumn(
                state = listState,
                verticalArrangement = Arrangement.spacedBy(8.dp),
                contentPadding = PaddingValues(12.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                items(entries.size, key = { idx ->
                    when (val e = entries[idx]) {
                        is TranscriptEntry.Pair -> e.id
                        is TranscriptEntry.Sep -> -e.id
                    }
                }) { idx ->
                    when (val entry = entries[idx]) {
                        is TranscriptEntry.Sep -> DashedSeparator(entry.time)
                        is TranscriptEntry.Pair -> ChatBubble(entry, accent)
                    }
                }
            }
        }
    }
}

@Composable
private fun ChatBubble(pair: TranscriptEntry.Pair, accent: Color) {
    val surfaceLow = MaterialTheme.colorScheme.surfaceContainerLow
    val backgroundColor by animateColorAsState(
        targetValue = when (pair.placement) {
            TranscriptBubblePlacement.CENTER -> lerp(surfaceLow, accent, 0.03f)
            TranscriptBubblePlacement.LEFT -> lerp(surfaceLow, accent, 0.06f)
            TranscriptBubblePlacement.RIGHT -> lerp(surfaceLow, accent, 0.14f)
        },
        label = "bubbleBackgroundColor",
    )
    val outputColor by animateColorAsState(
        targetValue = when (pair.placement) {
            TranscriptBubblePlacement.CENTER -> MaterialTheme.colorScheme.onSurfaceVariant
            TranscriptBubblePlacement.LEFT -> MaterialTheme.colorScheme.onSurface
            TranscriptBubblePlacement.RIGHT -> accent
        },
        label = "bubbleOutputColor",
    )
    val bottomStartRadius by animateDpAsState(
        targetValue = when (pair.placement) {
            TranscriptBubblePlacement.LEFT -> 6.dp
            else -> 18.dp
        },
        label = "bubbleBottomStartRadius",
    )
    val bottomEndRadius by animateDpAsState(
        targetValue = when (pair.placement) {
            TranscriptBubblePlacement.RIGHT -> 6.dp
            else -> 18.dp
        },
        label = "bubbleBottomEndRadius",
    )

    val translationAnim = remember(pair.id) { Animatable(0f) }
    val alpha = remember(pair.id) { Animatable(0f) }
    val scale = remember(pair.id) { Animatable(0.96f) }
    var shown by remember(pair.id) { mutableStateOf(false) }

    BoxWithConstraints(
        modifier = Modifier.fillMaxWidth(),
        contentAlignment = Alignment.Center,
    ) {
        val containerWidthPx = with(LocalDensity.current) { maxWidth.toPx() }
        var bubbleWidthPx by remember(pair.id) { mutableFloatStateOf(0f) }
        val travelPx = (containerWidthPx - bubbleWidthPx).coerceAtLeast(0f)
        val targetTranslationPx = when {
            containerWidthPx <= 0f || bubbleWidthPx <= 0f -> 0f
            pair.placement == TranscriptBubblePlacement.CENTER -> 0f
            pair.placement == TranscriptBubblePlacement.LEFT -> -travelPx / 2f
            else -> travelPx / 2f
        }

        LaunchedEffect(pair.id) {
            translationAnim.snapTo(0f)
            alpha.snapTo(0f)
            scale.snapTo(0.96f)
            alpha.animateTo(1f, tween(140))
            scale.animateTo(1f, tween(140))
            shown = true
        }

        LaunchedEffect(pair.id, pair.placement, targetTranslationPx) {
            if (!shown && targetTranslationPx != 0f) {
                kotlinx.coroutines.delay(70)
            }
            if (targetTranslationPx == 0f) {
                translationAnim.animateTo(0f, tween(180))
            } else {
                translationAnim.animateTo(targetTranslationPx, tween(320))
            }
        }

        Card(
            modifier = Modifier
                .widthIn(max = 280.dp)
                .onGloballyPositioned { bubbleWidthPx = it.size.width.toFloat() }
                .graphicsLayer {
                    this.translationX = translationAnim.value
                    this.alpha = alpha.value
                    this.scaleX = scale.value
                    this.scaleY = scale.value
                },
            shape = RoundedCornerShape(18.dp, 18.dp, bottomEndRadius, bottomStartRadius),
            colors = CardDefaults.cardColors(
                containerColor = backgroundColor,
            ),
        ) {
            Column(modifier = Modifier.padding(10.dp, 8.dp)) {
                if (pair.input.isNotBlank()) {
                    Text(
                        pair.input,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (pair.output.isNotBlank()) {
                    Text(
                        pair.output,
                        style = MaterialTheme.typography.bodyLarge,
                        fontWeight = FontWeight.SemiBold,
                        color = outputColor,
                    )
                }
            }
        }
    }
}

@Composable
private fun DashedSeparator(time: String) {
    val color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        DashedLine(modifier = Modifier.weight(1f), color = color)
        Text(
            time,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
        )
        DashedLine(modifier = Modifier.weight(1f), color = color)
    }
}

@Composable
private fun DashedLine(modifier: Modifier, color: Color) {
    Canvas(modifier = modifier.height(1.dp)) {
        drawLine(
            color = color,
            start = Offset(0f, size.height / 2),
            end = Offset(size.width, size.height / 2),
            strokeWidth = 1.dp.toPx(),
            pathEffect = PathEffect.dashPathEffect(floatArrayOf(4.dp.toPx(), 4.dp.toPx())),
        )
    }
}

// ── Status Dot ──

@Composable
private fun StatusDot(connectionState: TranslationGummyConnectionState) {
    val color = when (connectionState) {
        TranslationGummyConnectionState.READY -> Color(0xFF4CAF50)
        TranslationGummyConnectionState.CONNECTING -> CoralAccent
        TranslationGummyConnectionState.RECONNECTING -> Color(0xFFFFC107)
        TranslationGummyConnectionState.ERROR -> Color(0xFFF44336)
        else -> MaterialTheme.colorScheme.outlineVariant
    }
    Canvas(modifier = Modifier.size(8.dp)) {
        drawCircle(color = color)
        if (connectionState == TranslationGummyConnectionState.READY) {
            drawCircle(color = color.copy(alpha = 0.3f), radius = size.minDimension * 0.8f)
        }
    }
}

// ── Compact Waveform ──

private const val WF_NUM_BARS = 22

@Composable
private fun CompactWaveform(
    connectionState: TranslationGummyConnectionState,
    level: Float,
    accent: Color,
    modifier: Modifier = Modifier,
) {
    // Real RMS-driven scrolling bars (matches Windows recording indicator pattern)
    val barHeights = remember { FloatArray(WF_NUM_BARS + 2) }
    var scrollProgress by remember { mutableFloatStateOf(0f) }
    var lastFrameNanos by remember { mutableLongStateOf(0L) }

    val gradient = Brush.verticalGradient(listOf(accent, accent.copy(alpha = 0.85f), accent.copy(alpha = 0.6f)))

    val displayLevel = when (connectionState) {
        TranslationGummyConnectionState.READY -> level.coerceAtLeast(0.02f)
        TranslationGummyConnectionState.CONNECTING -> 0.10f
        TranslationGummyConnectionState.RECONNECTING -> 0.08f
        else -> 0.01f
    }

    Canvas(modifier = modifier) {
        val now = System.nanoTime()
        val dt = if (lastFrameNanos == 0L) 0.016f else ((now - lastFrameNanos) / 1_000_000_000f).coerceAtMost(0.05f)
        lastFrameNanos = now

        // Scroll bars left-to-right
        scrollProgress += dt / 0.15f
        val barWidth = 3.dp.toPx()
        val barGap = 2.dp.toPx()
        val barSpacing = barWidth + barGap
        val fadeWidth = 10.dp.toPx()
        val minH = 3.dp.toPx()
        val maxH = size.height - 2.dp.toPx()

        while (scrollProgress >= 1f) {
            scrollProgress -= 1f
            // Shift bars left, push new bar with real RMS level
            for (i in 0 until barHeights.size - 1) {
                barHeights[i] = barHeights[i + 1]
            }
            val newH = (displayLevel * maxH * 4f + minH).coerceIn(minH, maxH)
            barHeights[barHeights.size - 1] = newH
        }

        val pixelOffset = scrollProgress * barSpacing

        for (i in barHeights.indices) {
            val h = barHeights[i]
            val x = i * barSpacing - pixelOffset
            if (x + barWidth < 0f || x > size.width) continue

            val leftDist = x.coerceAtLeast(0f)
            val rightDist = (size.width - x - barWidth).coerceAtLeast(0f)
            val alpha = (minOf(leftDist, rightDist) / fadeWidth).coerceIn(0f, 1f)

            if (alpha > 0.01f && h > 0.5f) {
                drawRoundRect(
                    brush = gradient,
                    topLeft = Offset(x, (size.height - h) / 2f),
                    size = Size(barWidth, h),
                    cornerRadius = CornerRadius(barWidth / 2f),
                    alpha = alpha,
                )
            }
        }
    }
}

// ── MorphBadge (local M3E shape badge) ──

@Composable
private fun MorphBadge(
    from: RoundedPolygon,
    to: RoundedPolygon,
    progress: Float,
    containerColor: Color,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    val morph = remember(from, to) { Morph(from, to) }
    Box(
        modifier = modifier.drawWithCache {
            val path = morphToPath(morph, progress, size)
            onDrawBehind { drawPath(path, containerColor) }
        },
        contentAlignment = Alignment.Center,
        content = content,
    )
}

private fun morphToPath(morph: Morph, progress: Float, size: Size): androidx.compose.ui.graphics.Path {
    val composePath = morph.toPath(progress)
    val aPath = composePath.asAndroidPath()
    val bounds = android.graphics.RectF()
    aPath.computeBounds(bounds, true)
    val pw = maxOf(bounds.width(), 1f)
    val ph = maxOf(bounds.height(), 1f)
    val scale = minOf(size.width / pw, size.height / ph) * 0.9f
    val matrix = android.graphics.Matrix()
    matrix.postTranslate(-bounds.centerX(), -bounds.centerY())
    matrix.postScale(scale, scale)
    matrix.postTranslate(size.width / 2f, size.height / 2f)
    aPath.transform(matrix)
    return aPath.asComposePath()
}

private fun connectionStateLabel(
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
