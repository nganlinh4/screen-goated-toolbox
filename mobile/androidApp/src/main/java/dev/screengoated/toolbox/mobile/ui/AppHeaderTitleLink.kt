package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import kotlinx.coroutines.delay

private const val HeaderCreditUrl = "https://github.com/nganlinh4/screen-goated-toolbox"
private const val HeaderCreditText = "by nganlinh4"

@Composable
internal fun AppHeaderTitleLink(
    title: String,
    modifier: Modifier = Modifier,
) {
    val uriHandler = LocalUriHandler.current
    var showCredit by remember { mutableStateOf(false) }
    val interactionSource = remember { MutableInteractionSource() }
    val scale by animateFloatAsState(
        targetValue = if (showCredit) 1.07f else 1f,
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessMediumLow,
        ),
        label = "header-title-scale",
    )

    LaunchedEffect(showCredit) {
        if (showCredit) {
            delay(1000)
            showCredit = false
        }
    }

    AnimatedContent(
        targetState = showCredit,
        transitionSpec = {
            fadeIn(
                animationSpec = spring(
                    dampingRatio = Spring.DampingRatioMediumBouncy,
                    stiffness = Spring.StiffnessMediumLow,
                ),
            ) togetherWith fadeOut()
        },
        label = "header-title-credit",
        modifier = modifier
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .clickable(
                interactionSource = interactionSource,
                indication = null,
            ) {
                if (showCredit) {
                    uriHandler.openUri(HeaderCreditUrl)
                } else {
                    showCredit = true
                }
            }
            .semantics { role = Role.Button },
    ) { creditVisible ->
        Text(
            text = if (creditVisible) HeaderCreditText else title,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = if (creditVisible) FontWeight.SemiBold else null,
            color = if (creditVisible) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
