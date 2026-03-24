package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.KeyboardArrowUp
import androidx.compose.material.icons.outlined.OpenInFull
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors
import kotlin.math.roundToInt

@Composable
internal fun OverlayPane(
    modifier: Modifier,
    accentColor: Color,
    title: @Composable () -> Unit,
    headerCollapsed: Boolean,
    onHeaderToggle: () -> Unit,
    onWindowDrag: (Int, Int) -> Unit,
    controls: @Composable RowScope.() -> Unit,
    content: @Composable () -> Unit,
) {
    val sgtColors = MaterialTheme.sgtColors
    Column(
        modifier = modifier
            .shadow(18.dp, MaterialTheme.shapes.extraSmall, clip = false)
            .clip(MaterialTheme.shapes.extraSmall)
            .background(sgtColors.overlayBackground)
            .border(
                border = BorderStroke(1.dp, SolidColor(accentColor.copy(alpha = 0.35f))),
                shape = MaterialTheme.shapes.extraSmall,
            )
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        if (!headerCollapsed) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    modifier = Modifier
                        .weight(1f)
                        .pointerInput(Unit) {
                            detectDragGestures { change, dragAmount ->
                                change.consume()
                                onWindowDrag(
                                    dragAmount.x.roundToInt(),
                                    dragAmount.y.roundToInt(),
                                )
                            }
                        },
                    contentAlignment = Alignment.CenterStart,
                ) {
                    title()
                }
                Row(
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    content = controls,
                )
            }
        }
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = if (headerCollapsed) 0.dp else 2.dp),
            contentAlignment = Alignment.Center,
        ) {
            IconButton(
                onClick = onHeaderToggle,
                modifier = Modifier.size(20.dp),
            ) {
                Icon(
                    imageVector = Icons.Outlined.KeyboardArrowUp,
                    contentDescription = "Toggle header",
                    tint = sgtColors.overlayResizeHandle,
                    modifier = Modifier.size(14.dp),
                )
            }
        }
        Box(
            modifier = Modifier
                .fillMaxSize()
                .defaultMinSize(minHeight = 110.dp),
        ) {
            content()
        }
    }
}

@Composable
internal fun ListeningTitle() {
    val transition = rememberInfiniteTransition(label = "volume-bars")
    val bars = listOf(
        transition.animateFloat(
            initialValue = 9f,
            targetValue = 18f,
            animationSpec = infiniteRepeatable(tween(520), RepeatMode.Reverse),
            label = "bar1",
        ),
        transition.animateFloat(
            initialValue = 13f,
            targetValue = 22f,
            animationSpec = infiniteRepeatable(tween(700), RepeatMode.Reverse),
            label = "bar2",
        ),
        transition.animateFloat(
            initialValue = 8f,
            targetValue = 17f,
            animationSpec = infiniteRepeatable(tween(640), RepeatMode.Reverse),
            label = "bar3",
        ),
    )

    val sgtColors = MaterialTheme.sgtColors
    Row(
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        repeat(10) { index ->
            val animatedHeight by bars[index % bars.size]
            Spacer(
                modifier = Modifier
                    .size(width = 4.dp, height = animatedHeight.dp)
                    .clip(CircleShape)
                    .background(
                        Brush.verticalGradient(
                            listOf(sgtColors.waveformGradientStart, sgtColors.waveformGradientEnd),
                        ),
                    ),
            )
        }
    }
}

@Composable
internal fun OverlayTextBody(
    text: String,
    placeholder: String,
    fontSizeSp: Float,
) {
    val sgtColors = MaterialTheme.sgtColors
    val scrollState = rememberScrollState()
    Text(
        text = text.ifBlank { placeholder },
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(scrollState)
            .padding(top = 4.dp, bottom = 8.dp),
        color = if (text.isBlank()) sgtColors.overlayTextInactive else sgtColors.overlayTextActive,
        fontStyle = if (text.isBlank()) FontStyle.Italic else FontStyle.Normal,
        fontSize = fontSizeSp.sp,
        lineHeight = (fontSizeSp * 1.45f).sp,
    )
}

@Composable
internal fun OverlayTranslationBody(
    committedTranslation: String,
    liveTranslation: String,
    placeholder: String,
    fontSizeSp: Float,
) {
    val sgtColors = MaterialTheme.sgtColors
    val hasContent = committedTranslation.isNotBlank() || liveTranslation.isNotBlank()
    val scrollState = rememberScrollState()
    val text = if (!hasContent) {
        buildAnnotatedString { append(placeholder) }
    } else {
        buildAnnotatedString {
            if (committedTranslation.isNotBlank()) {
                pushStyle(
                    SpanStyle(
                        color = sgtColors.overlayCommittedText,
                        fontWeight = FontWeight.Light,
                    ),
                )
                append(committedTranslation.trim())
                pop()
            }
            if (liveTranslation.isNotBlank()) {
                if (committedTranslation.isNotBlank()) {
                    append(" ")
                }
                pushStyle(
                    SpanStyle(
                        color = sgtColors.overlayTextActive,
                        fontWeight = FontWeight.Medium,
                    ),
                )
                append(liveTranslation.trim())
                pop()
            }
        }
    }

    Text(
        text = text,
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(scrollState)
            .padding(top = 4.dp, bottom = 8.dp),
        color = if (hasContent) Color.Unspecified else sgtColors.overlayTextInactive,
        fontStyle = if (hasContent) FontStyle.Normal else FontStyle.Italic,
        fontSize = fontSizeSp.sp,
        lineHeight = (fontSizeSp * 1.45f).sp,
    )
}

@Composable
internal fun OverlayActionButton(
    icon: ImageVector,
    tint: Color,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    val sgtColors = MaterialTheme.sgtColors
    Surface(
        modifier = Modifier.size(28.dp),
        color = sgtColors.overlayActionButtonBg,
        shape = CircleShape,
        enabled = enabled,
        onClick = onClick,
    ) {
        Box(contentAlignment = Alignment.Center) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = if (enabled) tint else tint.copy(alpha = 0.35f),
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

@Composable
internal fun OverlayVisibilityButton(
    icon: ImageVector,
    tint: Color,
    active: Boolean,
    onClick: () -> Unit,
) {
    Surface(
        modifier = Modifier.size(28.dp),
        color = Color.Transparent,
        shape = RoundedCornerShape(8.dp),
        onClick = onClick,
    ) {
        Box(contentAlignment = Alignment.Center) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = if (active) tint else tint.copy(alpha = 0.35f),
                modifier = Modifier.size(18.dp),
            )
        }
    }
}

@Composable
internal fun OverlayIconBadge(
    icon: ImageVector,
    tint: Color,
) {
    Box(
        modifier = Modifier.size(28.dp),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = tint,
            modifier = Modifier.size(18.dp),
        )
    }
}

@Composable
internal fun OverlayResizeHandle(
    modifier: Modifier = Modifier,
    onWindowResize: (Int, Int) -> Unit,
) {
    val sgtColors = MaterialTheme.sgtColors
    Box(
        modifier = modifier
            .size(30.dp)
            .pointerInput(Unit) {
                detectDragGestures { change, dragAmount ->
                    change.consume()
                    onWindowResize(
                        dragAmount.x.roundToInt(),
                        dragAmount.y.roundToInt(),
                    )
                }
            },
        contentAlignment = Alignment.BottomEnd,
    ) {
        Icon(
            imageVector = Icons.Outlined.OpenInFull,
            contentDescription = "Resize window",
            tint = sgtColors.overlayResizeHandle,
            modifier = Modifier.size(18.dp),
        )
    }
}
