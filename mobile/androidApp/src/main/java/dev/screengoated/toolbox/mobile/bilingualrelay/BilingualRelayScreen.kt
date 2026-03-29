@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.bilingualrelay

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
fun BilingualRelayScreen(
    locale: MobileLocaleText,
    onBack: () -> Unit,
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
        val canRecord = granted[Manifest.permission.RECORD_AUDIO] == true
        if (canRecord && state.appliedConfig.isValid()) {
            BilingualRelayService.start(context)
        }
    }

    fun ensureStarted(forceRestart: Boolean = false) {
        val hasRecordPermission = ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.RECORD_AUDIO,
        ) == PackageManager.PERMISSION_GRANTED

        if (hasRecordPermission) {
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

    Scaffold(
        topBar = {
            TopAppBar(
                title = {},
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = null)
                    }
                },
                actions = {
                    if (state.dirty) {
                        Button(
                            onClick = {
                                repository.applyDraft()
                                ensureStarted(forceRestart = true)
                            },
                            enabled = state.draftConfig.isValid(),
                        ) {
                            Text(locale.bilingualRelayApply)
                        }
                        Spacer(Modifier.size(8.dp))
                    }
                    Button(
                        onClick = {
                            if (state.isRunning) {
                                BilingualRelayService.stop(context)
                            } else {
                                ensureStarted()
                            }
                        },
                        enabled = state.isRunning || state.appliedConfig.isValid(),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = MaterialTheme.colorScheme.secondaryContainer,
                            contentColor = MaterialTheme.colorScheme.onSecondaryContainer,
                        ),
                    ) {
                        Icon(
                            if (state.isRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(Modifier.size(6.dp))
                        Text(if (state.isRunning) locale.bilingualRelayStop else locale.bilingualRelayStart)
                    }
                },
            )
        },
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            ProfileCard(
                title = locale.bilingualRelayFirstProfile,
                profile = state.draftConfig.first,
                languageLabel = locale.bilingualRelayLanguageLabel,
                accentLabel = locale.bilingualRelayAccentLabel,
                toneLabel = locale.bilingualRelayToneLabel,
                onChanged = { updated ->
                    repository.updateDraft { it.copy(first = updated) }
                },
            )

            ProfileCard(
                title = locale.bilingualRelaySecondProfile,
                profile = state.draftConfig.second,
                languageLabel = locale.bilingualRelayLanguageLabel,
                accentLabel = locale.bilingualRelayAccentLabel,
                toneLabel = locale.bilingualRelayToneLabel,
                onChanged = { updated ->
                    repository.updateDraft { it.copy(second = updated) }
                },
            )

            state.lastError?.takeIf(String::isNotBlank)?.let { error ->
                Text(
                    text = error,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }

            Card(
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
                ),
                modifier = Modifier.weight(1f),
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(12.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Text(
                        text = locale.bilingualRelayTranscriptTitle,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    if (state.transcripts.isEmpty()) {
                        Box(
                            modifier = Modifier.fillMaxSize(),
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(
                                text = locale.bilingualRelayNoTranscriptYet,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    } else {
                        LazyColumn(
                            state = transcriptListState,
                            verticalArrangement = Arrangement.spacedBy(8.dp),
                            modifier = Modifier.fillMaxSize(),
                        ) {
                            items(state.transcripts, key = { it.id }) { item ->
                                val chipText = if (item.role == BilingualRelayTranscriptRole.INPUT) {
                                    locale.bilingualRelayInputChip
                                } else {
                                    locale.bilingualRelayOutputChip
                                }
                                Card(
                                    colors = CardDefaults.cardColors(
                                        containerColor = MaterialTheme.colorScheme.surfaceContainer,
                                    ),
                                ) {
                                    Row(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .padding(10.dp),
                                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                                        verticalAlignment = Alignment.Top,
                                    ) {
                                        AssistChip(
                                            onClick = {},
                                            enabled = false,
                                            label = { Text(chipText) },
                                        )
                                        Text(
                                            item.text,
                                            style = MaterialTheme.typography.bodyLarge,
                                            modifier = Modifier.weight(1f),
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }

            ReadyVisualizer(
                label = connectionStateLabel(state.connectionState, locale),
                state = state.connectionState,
                level = state.visualizerLevel,
            )
        }
    }
}

@Composable
private fun ProfileCard(
    title: String,
    profile: BilingualRelayLanguageProfile,
    languageLabel: String,
    accentLabel: String,
    toneLabel: String,
    onChanged: (BilingualRelayLanguageProfile) -> Unit,
) {
    Card(
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainer,
        ),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedTextField(
                    value = profile.language,
                    onValueChange = { onChanged(profile.copy(language = it)) },
                    label = { Text(languageLabel) },
                    modifier = Modifier.weight(1f),
                    singleLine = true,
                )
                OutlinedTextField(
                    value = profile.accent,
                    onValueChange = { onChanged(profile.copy(accent = it)) },
                    label = { Text(accentLabel) },
                    modifier = Modifier.weight(1f),
                    singleLine = true,
                )
                OutlinedTextField(
                    value = profile.tone,
                    onValueChange = { onChanged(profile.copy(tone = it)) },
                    label = { Text(toneLabel) },
                    modifier = Modifier.weight(1f),
                    singleLine = true,
                )
            }
        }
    }
}

@Composable
private fun ReadyVisualizer(
    label: String,
    state: BilingualRelayConnectionState,
    level: Float,
) {
    val colors = when (state) {
        BilingualRelayConnectionState.READY -> listOf(Color(0xFF00A8E0), Color(0xFF00C8FF), Color(0xFF40E0FF))
        BilingualRelayConnectionState.CONNECTING -> listOf(Color(0xFF9F7AEA), Color(0xFF805AD5), Color(0xFFB794F4))
        BilingualRelayConnectionState.RECONNECTING -> listOf(Color(0xFFFFD700), Color(0xFFFFA500), Color(0xFFFFDEAD))
        BilingualRelayConnectionState.ERROR -> listOf(Color(0xFFC62828), Color(0xFFE57373), Color(0xFFFFCDD2))
        BilingualRelayConnectionState.NOT_CONFIGURED -> listOf(Color(0xFF666666), Color(0xFF888888), Color(0xFFAAAAAA))
        BilingualRelayConnectionState.STOPPED -> listOf(Color(0xFF666666), Color(0xFF888888), Color(0xFFAAAAAA))
    }
    val transition = rememberInfiniteTransition(label = "relay-visualizer")
    val phase by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 1200, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "relay-visualizer-phase",
    )
    val animatedLevel by animateFloatAsState(
        targetValue = level.coerceIn(0f, 1f),
        animationSpec = tween(180),
        label = "relay-visualizer-level",
    )
    Card(
        colors = CardDefaults.cardColors(containerColor = colors[0].copy(alpha = 0.12f)),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(120.dp)
                .background(Color.Transparent),
        ) {
            Canvas(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 16.dp, vertical = 14.dp),
            ) {
                val barWidth = 8.dp.toPx()
                val barGap = 6.dp.toPx()
                val barSpacing = barWidth + barGap
                val visibleBars = ((size.width / barSpacing).toInt() + 3).coerceAtLeast(12)
                val scroll = phase * barSpacing
                val gradient = Brush.verticalGradient(colors)
                val displayLevel = when (state) {
                    BilingualRelayConnectionState.READY -> animatedLevel.coerceAtLeast(0.05f)
                    BilingualRelayConnectionState.CONNECTING -> 0.10f + 0.12f * kotlin.math.abs(kotlin.math.sin((phase * Math.PI.toFloat() * 2f).toDouble())).toFloat()
                    BilingualRelayConnectionState.RECONNECTING -> 0.08f + 0.10f * kotlin.math.abs(kotlin.math.sin((phase * Math.PI.toFloat() * 4f).toDouble())).toFloat()
                    BilingualRelayConnectionState.ERROR -> 0.05f
                    BilingualRelayConnectionState.NOT_CONFIGURED -> 0.02f
                    BilingualRelayConnectionState.STOPPED -> 0.02f
                }

                for (index in 0 until visibleBars) {
                    val x = index * barSpacing - scroll
                    if (x + barWidth < 0f || x > size.width) {
                        continue
                    }
                    val wave = kotlin.math.abs(
                        kotlin.math.sin(((index * 0.48f) + (phase * Math.PI.toFloat() * 2f)).toDouble()),
                    ).toFloat()
                    val minHeight = 8.dp.toPx()
                    val maxHeight = size.height - 8.dp.toPx()
                    val barHeight = (minHeight + (maxHeight - minHeight) * (0.18f + displayLevel * wave))
                        .coerceIn(minHeight, maxHeight)
                    val edgeDistance = kotlin.math.min(x.coerceAtLeast(0f), (size.width - x - barWidth).coerceAtLeast(0f))
                    val alpha = (edgeDistance / 24.dp.toPx()).coerceIn(0.25f, 1f)
                    drawRoundRect(
                        brush = gradient,
                        topLeft = Offset(x, (size.height - barHeight) / 2f),
                        size = Size(barWidth, barHeight),
                        cornerRadius = CornerRadius(barWidth / 2f, barWidth / 2f),
                        alpha = alpha,
                    )
                }
            }
            Text(
                text = label,
                style = MaterialTheme.typography.headlineSmall,
                color = colors[1],
                fontWeight = FontWeight.Bold,
                modifier = Modifier.align(Alignment.Center),
            )
        }
    }
}

private fun connectionStateLabel(
    state: BilingualRelayConnectionState,
    locale: MobileLocaleText,
): String {
    return when (state) {
        BilingualRelayConnectionState.NOT_CONFIGURED -> locale.bilingualRelayStatusNotConfigured
        BilingualRelayConnectionState.CONNECTING -> locale.bilingualRelayStatusConnecting
        BilingualRelayConnectionState.READY -> locale.bilingualRelayStatusReady
        BilingualRelayConnectionState.RECONNECTING -> locale.bilingualRelayStatusReconnecting
        BilingualRelayConnectionState.ERROR -> locale.bilingualRelayStatusError
        BilingualRelayConnectionState.STOPPED -> locale.bilingualRelayStatusStopped
    }
}
