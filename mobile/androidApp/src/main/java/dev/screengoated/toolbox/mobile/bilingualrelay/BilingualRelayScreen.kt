@file:OptIn(
    androidx.compose.material3.ExperimentalMaterial3Api::class,
    androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class,
)

package dev.screengoated.toolbox.mobile.bilingualrelay

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Stop
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
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

private val CoralAccent = Color(0xFFFF7387)

@Composable
fun BilingualRelayScreen(
    locale: MobileLocaleText,
    onBack: () -> Unit,
    onNavigateToTtsSettings: () -> Unit = {},
) {
    val context = LocalContext.current
    val repository = remember(context) {
        (context.applicationContext as SgtMobileApplication).appContainer.bilingualRelayRepository
    }
    val state by repository.state.collectAsState()
    var autoStartAttempted by remember { mutableStateOf(false) }

    val permissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestMultiplePermissions(),
    ) { granted ->
        if (granted[Manifest.permission.RECORD_AUDIO] == true && state.appliedConfig.isValid()) {
            BilingualRelayService.start(context)
        }
    }

    fun ensureStarted(forceRestart: Boolean = false) {
        val hasPermission = ContextCompat.checkSelfPermission(
            context, Manifest.permission.RECORD_AUDIO,
        ) == PackageManager.PERMISSION_GRANTED
        if (hasPermission) {
            BilingualRelayService.start(context, restart = forceRestart)
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
                        Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = null)
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
                        Text(
                            connectionStateLabel(state.connectionState, locale),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                },
                actions = {
                    IconButton(onClick = onNavigateToTtsSettings) {
                        Surface(
                            shape = MaterialTheme.shapes.small,
                            color = CoralAccent.copy(alpha = 0.12f),
                            modifier = Modifier.size(32.dp),
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    Icons.Rounded.Settings,
                                    contentDescription = "TTS Settings",
                                    tint = CoralAccent,
                                    modifier = Modifier.size(18.dp),
                                )
                            }
                        }
                    }
                    AnimatedVisibility(
                        visible = state.dirty,
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
                            Text(locale.bilingualRelayApply, style = MaterialTheme.typography.labelMedium)
                        }
                    }
                    FilledTonalButton(
                        onClick = {
                            if (state.isRunning) {
                                BilingualRelayService.stop(context)
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
                        modifier = Modifier.height(36.dp),
                    ) {
                        Icon(
                            if (state.isRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(4.dp))
                        Text(
                            if (state.isRunning) locale.bilingualRelayStop else locale.bilingualRelayStart,
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
            RelayLanguageCard(
                number = "1",
                title = locale.bilingualRelayFirstProfile,
                profile = state.draftConfig.first,
                languagePlaceholder = locale.bilingualRelayLanguageLabel,
                accentPlaceholder = locale.bilingualRelayAccentLabel,
                tonePlaceholder = locale.bilingualRelayToneLabel,
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
            RelayLanguageCard(
                number = "2",
                title = locale.bilingualRelaySecondProfile,
                profile = state.draftConfig.second,
                languagePlaceholder = locale.bilingualRelayLanguageLabel,
                accentPlaceholder = locale.bilingualRelayAccentLabel,
                tonePlaceholder = locale.bilingualRelayToneLabel,
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
                emptyLabel = locale.bilingualRelayNoTranscriptYet,
                inputChip = locale.bilingualRelayInputChip,
                outputChip = locale.bilingualRelayOutputChip,
                accent = CoralAccent,
                listState = transcriptListState,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

// ── Language Card ──

@Composable
private fun RelayLanguageCard(
    number: String,
    title: String,
    profile: BilingualRelayLanguageProfile,
    languagePlaceholder: String,
    accentPlaceholder: String,
    tonePlaceholder: String,
    accent: Color,
    onChanged: ((BilingualRelayLanguageProfile) -> Unit)? = null,
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
                // Number badge
                Surface(
                    shape = MaterialTheme.shapes.small,
                    color = accent.copy(alpha = 0.15f),
                    modifier = Modifier.size(28.dp),
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Text(
                            number,
                            style = MaterialTheme.typography.labelMedium,
                            fontWeight = FontWeight.Bold,
                            color = accent,
                        )
                    }
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
                    colors = relayTextFieldColors(accent),
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
                    colors = relayTextFieldColors(accent),
                )
                OutlinedTextField(
                    value = profile.tone,
                    onValueChange = onToneChanged,
                    placeholder = { Text(tonePlaceholder, style = MaterialTheme.typography.bodySmall) },
                    modifier = Modifier.weight(1f).defaultMinSize(minHeight = 44.dp),
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodySmall,
                    shape = MaterialTheme.shapes.large,
                    colors = relayTextFieldColors(accent),
                )
            }
        }
    }
}

@Composable
private fun relayTextFieldColors(accent: Color): TextFieldColors {
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
    data class Pair(val id: Long, val input: String, val output: String, val lang: String, val isLeft: Boolean) : TranscriptEntry()
    data class Sep(val id: Long, val time: String) : TranscriptEntry()
}

private fun groupEntries(items: List<BilingualRelayTranscriptItem>): List<TranscriptEntry> {
    val entries = mutableListOf<TranscriptEntry>()
    var i = 0
    while (i < items.size) {
        val item = items[i]
        if (item.role == BilingualRelayTranscriptRole.SEPARATOR) {
            entries += TranscriptEntry.Sep(item.id, item.text)
            i++
            continue
        }
        if (item.role == BilingualRelayTranscriptRole.INPUT) {
            val next = items.getOrNull(i + 1)
            if (next != null && next.role == BilingualRelayTranscriptRole.OUTPUT) {
                entries += TranscriptEntry.Pair(item.id, item.text, next.text, next.lang, isLeft = true)
                i += 2
            } else {
                entries += TranscriptEntry.Pair(item.id, item.text, "", item.lang, isLeft = true)
                i++
            }
        } else {
            entries += TranscriptEntry.Pair(item.id, "", item.text, item.lang, isLeft = true)
            i++
        }
    }
    // Determine alignment by first detected lang
    val firstLang = entries.filterIsInstance<TranscriptEntry.Pair>().firstOrNull { it.lang.isNotBlank() }?.lang.orEmpty()
    if (firstLang.isNotBlank()) {
        for (idx in entries.indices) {
            val e = entries[idx]
            if (e is TranscriptEntry.Pair) {
                entries[idx] = e.copy(isLeft = e.lang.isBlank() || e.lang == firstLang)
            }
        }
    }
    return entries
}

@Composable
private fun TranscriptCard(
    transcripts: List<BilingualRelayTranscriptItem>,
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
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = if (pair.isLeft) Arrangement.Start else Arrangement.End,
    ) {
        Card(
            modifier = Modifier.widthIn(max = 280.dp),
            shape = if (pair.isLeft) {
                RoundedCornerShape(18.dp, 18.dp, 18.dp, 6.dp)
            } else {
                RoundedCornerShape(18.dp, 18.dp, 6.dp, 18.dp)
            },
            colors = CardDefaults.cardColors(
                containerColor = if (pair.isLeft) {
                    lerp(surfaceLow, accent, 0.06f)
                } else {
                    lerp(surfaceLow, accent, 0.14f)
                },
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
                        color = if (pair.isLeft) MaterialTheme.colorScheme.onSurface else accent,
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
private fun StatusDot(connectionState: BilingualRelayConnectionState) {
    val color = when (connectionState) {
        BilingualRelayConnectionState.READY -> Color(0xFF4CAF50)
        BilingualRelayConnectionState.CONNECTING -> CoralAccent
        BilingualRelayConnectionState.RECONNECTING -> Color(0xFFFFC107)
        BilingualRelayConnectionState.ERROR -> Color(0xFFF44336)
        else -> MaterialTheme.colorScheme.outlineVariant
    }
    Canvas(modifier = Modifier.size(8.dp)) {
        drawCircle(color = color)
        if (connectionState == BilingualRelayConnectionState.READY) {
            drawCircle(color = color.copy(alpha = 0.3f), radius = size.minDimension * 0.8f)
        }
    }
}

// ── Compact Waveform ──

private const val WF_NUM_BARS = 22

@Composable
private fun CompactWaveform(
    connectionState: BilingualRelayConnectionState,
    level: Float,
    accent: Color,
    modifier: Modifier = Modifier,
) {
    // Real RMS-driven scrolling bars (matches Windows recording indicator pattern)
    val barHeights = remember { FloatArray(WF_NUM_BARS + 2) { 0f } }
    var scrollProgress by remember { mutableFloatStateOf(0f) }
    var lastFrameNanos by remember { mutableLongStateOf(0L) }

    val gradient = Brush.verticalGradient(listOf(accent, accent.copy(alpha = 0.85f), accent.copy(alpha = 0.6f)))

    val displayLevel = when (connectionState) {
        BilingualRelayConnectionState.READY -> level.coerceAtLeast(0.02f)
        BilingualRelayConnectionState.CONNECTING -> 0.10f
        BilingualRelayConnectionState.RECONNECTING -> 0.08f
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

private fun connectionStateLabel(
    state: BilingualRelayConnectionState,
    locale: MobileLocaleText,
): String = when (state) {
    BilingualRelayConnectionState.NOT_CONFIGURED -> locale.bilingualRelayStatusNotConfigured
    BilingualRelayConnectionState.CONNECTING -> locale.bilingualRelayStatusConnecting
    BilingualRelayConnectionState.READY -> locale.bilingualRelayStatusReady
    BilingualRelayConnectionState.RECONNECTING -> locale.bilingualRelayStatusReconnecting
    BilingualRelayConnectionState.ERROR -> locale.bilingualRelayStatusError
    BilingualRelayConnectionState.STOPPED -> locale.bilingualRelayStatusStopped
}
