@file:OptIn(androidx.compose.ui.text.ExperimentalTextApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

data class NodePosition(
    val id: String,
    val x: Float,
    val y: Float,
    val block: ProcessingBlock,
)

data class Connection(
    val fromNodeId: String,
    val toNodeId: String,
)

data class NodeGraphState(
    val nodes: List<NodePosition>,
    val connections: List<Connection>,
)

// ---------------------------------------------------------------------------
// Sizing / layout constants (dp)
// ---------------------------------------------------------------------------

internal val NODE_WIDTH_DP = 220.dp
internal val PIN_RADIUS_DP = 7.dp
internal val PIN_HIT_RADIUS_DP = 30.dp
internal val GRID_SPACING_DP = 24.dp
internal const val GRID_DOT_RADIUS = 1.5f
internal const val MIN_ZOOM = 0.25f
internal const val MAX_ZOOM = 2.5f
internal const val DEFAULT_NODE_HEIGHT_PX = 600f // generous estimate for nodes with prompts
internal const val FIT_PADDING = 40f // px padding around auto-fit content

// Layout gap between nodes (fraction of node dimension)
internal const val LAYOUT_GAP_X_RATIO = 0.6f  // horizontal gap = 60% of node width
internal const val LAYOUT_GAP_Y_RATIO = 0.25f // vertical gap = 25% of node height

// Pin Y offset: header(5dp) + padding(6dp) + pin center(7dp) ≈ 18dp from node top
internal const val PIN_Y_OFFSET_PX = 50f // ~18dp at typical density, will be scaled

internal val PIN_INPUT_COLOR = Color(0xFF66BB6A)
internal val PIN_OUTPUT_COLOR = Color(0xFF42A5F5)
internal val WIRE_DRAG_COLOR = Color(0xFFFFAB40)

// ---------------------------------------------------------------------------
// Shared fonts / language list (lazy singletons)
// ---------------------------------------------------------------------------

/** Google Sans Flex at wdth=75 for condensed descriptive text. */
internal val condensedFontFamily: androidx.compose.ui.text.font.FontFamily by lazy {
    androidx.compose.ui.text.font.FontFamily(
        androidx.compose.ui.text.font.Font(
            resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
            variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                androidx.compose.ui.text.font.FontVariation.Setting("wdth", 75f),
            ),
        ),
    )
}

/** All ISO 639-1 language names, sorted — matches Windows get_all_languages() from isolang crate. */
internal val ALL_ISO_LANGUAGES: List<String> by lazy {
    java.util.Locale.getISOLanguages()
        .mapNotNull { code ->
            val loc = java.util.Locale.forLanguageTag(code)
            loc.getDisplayLanguage(java.util.Locale.ENGLISH).takeIf { it.isNotBlank() && it != code }
        }
        .distinct()
        .sorted()
}
