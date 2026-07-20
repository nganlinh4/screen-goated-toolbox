@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class, ExperimentalTextApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.annotation.DrawableRes
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors

private val AppCardLandscapeIconSize = 40.dp
private val AppCardPortraitIconSize = 44.dp

@Composable
internal fun LiveTranslateCarouselTile(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
) {
    val isRunning = state.phase in setOf(
        SessionPhase.STARTING, SessionPhase.LISTENING, SessionPhase.TRANSLATING,
    )
    SessionAppCarouselTile(
        slot = appSlots[0],
        title = locale.shellLiveTitle,
        drawableRes = LiveTranslateVisuals.icon,
        isRunning = isRunning,
        onSessionToggle = onSessionToggle,
        canToggle = canToggle,
        turnOnLabel = locale.turnOn,
        turnOffLabel = locale.turnOff,
        toggleTag = "live-translate-toggle",
    )
}

@Composable
internal fun SessionAppCarouselTile(
    slot: AppSlot,
    title: String,
    @DrawableRes drawableRes: Int,
    isRunning: Boolean,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    turnOnLabel: String,
    turnOffLabel: String,
    toggleTag: String,
) {
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val slotColor = slot.colorToken(MaterialTheme.sgtColors)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(if (isRunning) slotColor.copy(alpha = 0.30f) else slotColor.copy(alpha = 0.15f)),
    ) {
        AnimatedShapesCanvas(
            color = slotColor,
            seed = slotColor.hashCode().toLong() xor 0x42L,
            isScrolling = isRunning, // morph faster when live translate is active
        )
        val stretchedFamily = remember {
            FontFamily(
                androidx.compose.ui.text.font.Font(
                    resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                    weight = FontWeight.Black,
                    variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                        androidx.compose.ui.text.font.FontVariation.weight(FontWeight.Black.weight),
                        androidx.compose.ui.text.font.FontVariation.Setting("ROND", 100f),
                        androidx.compose.ui.text.font.FontVariation.Setting("wdth", 125f),
                    ),
                ),
            )
        }
        if (isLandscape) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 18.dp, vertical = 16.dp),
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxSize(),
                    verticalArrangement = Arrangement.SpaceBetween,
                ) {
                    Icon(
                        painterResource(drawableRes),
                        contentDescription = null,
                        tint = slotColor,
                        modifier = Modifier.size(AppCardLandscapeIconSize),
                    )
                    Text(
                        text = title,
                        fontFamily = stretchedFamily,
                        fontWeight = FontWeight.Black,
                        fontSize = 22.sp,
                        lineHeight = 24.sp,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 3,
                    )
                }
                Button(
                    onClick = onSessionToggle,
                    enabled = canToggle,
                    shape = CircleShape,
                    colors = if (isRunning) {
                        ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
                    } else {
                        ButtonDefaults.buttonColors()
                    },
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .testTag(toggleTag),
                ) {
                    Icon(
                        painterResource(if (isRunning) R.drawable.ms_stop else R.drawable.ms_play_arrow),
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(Modifier.width(ButtonDefaults.IconSpacing))
                    Text(if (isRunning) turnOffLabel else turnOnLabel)
                }
            }
        } else {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 20.dp, vertical = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    painterResource(drawableRes),
                    contentDescription = null,
                    tint = slotColor,
                    modifier = Modifier.size(AppCardPortraitIconSize),
                )
                Spacer(Modifier.width(14.dp))
                Column(modifier = Modifier.weight(1f)) {
                    val words = title.split(" ", limit = 2)
                    if (words.isNotEmpty()) {
                        Text(
                            text = words[0],
                            fontFamily = stretchedFamily,
                            fontWeight = FontWeight.Black,
                            fontSize = 28.sp,
                            lineHeight = 32.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                    if (words.size > 1) {
                        Text(
                            text = words[1],
                            fontWeight = FontWeight.Bold,
                            fontSize = 26.sp,
                            lineHeight = 30.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
                Column(
                    horizontalAlignment = Alignment.End,
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Button(
                        onClick = onSessionToggle,
                        enabled = canToggle,
                        shape = CircleShape,
                        colors = if (isRunning) {
                            ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
                        } else {
                            ButtonDefaults.buttonColors()
                        },
                        modifier = Modifier.testTag(toggleTag),
                    ) {
                        Icon(
                            painterResource(if (isRunning) R.drawable.ms_stop else R.drawable.ms_play_arrow),
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(ButtonDefaults.IconSpacing))
                        Text(if (isRunning) turnOffLabel else turnOnLabel)
                    }
                }
            }
        }
    }
}

@Composable
internal fun AppTile(
    slot: AppSlot,
    title: String,
    @DrawableRes drawableRes: Int? = null,
) {
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val slotColor = slot.colorToken(MaterialTheme.sgtColors)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(slotColor.copy(alpha = 0.15f)),
    ) {
        AnimatedShapesCanvas(
            color = slotColor,
            seed = slotColor.hashCode().toLong(),
        )
        val stretchedFamily = remember {
            FontFamily(
                androidx.compose.ui.text.font.Font(
                    resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                    weight = FontWeight.Black,
                    variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                        androidx.compose.ui.text.font.FontVariation.weight(FontWeight.Black.weight),
                        androidx.compose.ui.text.font.FontVariation.Setting("ROND", 100f),
                        androidx.compose.ui.text.font.FontVariation.Setting("wdth", 125f),
                    ),
                ),
            )
        }
        if (isLandscape) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 18.dp, vertical = 16.dp),
                verticalArrangement = Arrangement.SpaceBetween,
            ) {
                if (drawableRes != null) {
                    Icon(painterResource(drawableRes), contentDescription = null, tint = slotColor, modifier = Modifier.size(AppCardLandscapeIconSize))
                }
                Text(
                    text = title,
                    fontFamily = stretchedFamily,
                    fontWeight = FontWeight.Black,
                    fontSize = 22.sp,
                    lineHeight = 24.sp,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 3,
                )
            }
        } else {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 20.dp, vertical = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (drawableRes != null) {
                    Icon(painterResource(drawableRes), contentDescription = null, tint = slotColor, modifier = Modifier.size(AppCardPortraitIconSize))
                    Spacer(Modifier.width(14.dp))
                }
                Column(modifier = Modifier.weight(1f)) {
                    val lines = remember(title) { portraitTitleLines(title) }
                    if (lines.isNotEmpty()) {
                        Text(
                            text = lines[0],
                            fontFamily = stretchedFamily,
                            fontWeight = FontWeight.Black,
                            fontSize = 28.sp,
                            lineHeight = 32.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                    if (lines.size > 1) {
                        Text(
                            text = lines[1],
                            fontWeight = FontWeight.Bold,
                            fontSize = 26.sp,
                            lineHeight = 30.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
            }
        }
    }
}

private fun portraitTitleLines(title: String): List<String> {
    val normalized = title.trim()
    if (normalized.isEmpty()) return emptyList()
    val words = normalized.split(Regex("\\s+")).filter { it.isNotBlank() }
    if (words.size <= 1) return listOf(normalized)

    var bestLeft = words.first()
    var bestRight = words.drop(1).joinToString(" ")
    var bestScore = scoreSplit(bestLeft, bestRight)

    for (splitIndex in 1 until words.lastIndex + 1) {
        val left = words.take(splitIndex).joinToString(" ")
        val right = words.drop(splitIndex).joinToString(" ")
        val score = scoreSplit(left, right)
        if (score < bestScore) {
            bestLeft = left
            bestRight = right
            bestScore = score
        }
    }

    return if (bestRight.isBlank()) listOf(bestLeft) else listOf(bestLeft, bestRight)
}

private fun scoreSplit(left: String, right: String): Int {
    val lengthGap = kotlin.math.abs(left.length - right.length)
    val maxLength = maxOf(left.length, right.length)
    return lengthGap * 10 + maxLength
}

@Composable
internal fun EmptyAppTile(slot: AppSlot) {
    val slotColor = slot.colorToken(MaterialTheme.sgtColors)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(slotColor.copy(alpha = 0.15f)),
    ) {
        AnimatedShapesCanvas(
            color = slotColor,
            seed = slotColor.hashCode().toLong(),
        )
    }
}
