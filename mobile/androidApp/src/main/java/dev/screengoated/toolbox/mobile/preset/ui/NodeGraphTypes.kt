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
// Shared fonts / icon assets (lazy singletons)
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

// Material Symbol: file_copy (rounded, 24px)
// SVG path from viewBox="0 -960 960 960", all Y coords shifted by +960
internal val FileCopyIcon: androidx.compose.ui.graphics.vector.ImageVector by lazy {
    androidx.compose.ui.graphics.vector.ImageVector.Builder(
        name = "FileCopy", defaultWidth = 24.dp, defaultHeight = 24.dp,
        viewportWidth = 960f, viewportHeight = 960f,
    ).apply {
        addPath(
            pathData = androidx.compose.ui.graphics.vector.PathData {
                // M760-200 → M760,760
                moveTo(760f, 760f); horizontalLineTo(320f)
                quadTo(287f, 760f, 263.5f, 736.5f); quadTo(240f, 713f, 240f, 680f)
                verticalLineTo(120f)
                quadTo(240f, 87f, 263.5f, 63.5f); quadTo(287f, 40f, 320f, 40f)
                horizontalLineTo(600f); lineTo(840f, 280f); verticalLineTo(680f)
                quadTo(840f, 713f, 816.5f, 736.5f); quadTo(793f, 760f, 760f, 760f)
                close()
                moveTo(560f, 320f); verticalLineTo(120f); horizontalLineTo(320f)
                verticalLineTo(680f); horizontalLineTo(760f); verticalLineTo(320f)
                horizontalLineTo(560f); close()
                moveTo(160f, 920f)
                quadTo(127f, 920f, 103.5f, 896.5f); quadTo(80f, 873f, 80f, 840f)
                verticalLineTo(280f); horizontalLineTo(160f); verticalLineTo(840f)
                horizontalLineTo(600f); verticalLineTo(920f); horizontalLineTo(160f)
                close()
                moveTo(320f, 120f); verticalLineTo(320f); verticalLineTo(120f)
                verticalLineTo(680f); verticalLineTo(120f); close()
            },
            fill = androidx.compose.ui.graphics.SolidColor(Color.Black),
        )
    }.build()
}

// Material Symbol: file_copy_off (rounded, 24px)
// SVG path from viewBox="0 -960 960 960", all Y coords shifted by +960
internal val FileCopyOffIcon: androidx.compose.ui.graphics.vector.ImageVector by lazy {
    androidx.compose.ui.graphics.vector.ImageVector.Builder(
        name = "FileCopyOff", defaultWidth = 24.dp, defaultHeight = 24.dp,
        viewportWidth = 960f, viewportHeight = 960f,
    ).apply {
        addPath(
            pathData = androidx.compose.ui.graphics.vector.PathData {
                moveTo(840f, 726f); lineTo(760f, 646f); verticalLineTo(320f)
                horizontalLineTo(560f); verticalLineTo(120f); horizontalLineTo(320f)
                verticalLineTo(206f); lineTo(240f, 126f); verticalLineTo(120f)
                quadTo(240f, 87f, 263.5f, 63.5f); quadTo(287f, 40f, 320f, 40f)
                horizontalLineTo(600f); lineTo(840f, 280f); verticalLineTo(726f)
                close()
                moveTo(320f, 680f); horizontalLineTo(568f); lineTo(320f, 432f)
                verticalLineTo(680f); close()
                moveTo(820f, 932f); lineTo(648f, 760f); horizontalLineTo(320f)
                quadTo(287f, 760f, 263.5f, 736.5f); quadTo(240f, 713f, 240f, 680f)
                verticalLineTo(352f); lineTo(28f, 140f); lineTo(84f, 84f)
                lineTo(876f, 876f); lineTo(820f, 932f); close()
                moveTo(540f, 383f); close()
                moveTo(444f, 556f); close()
                moveTo(160f, 920f)
                quadTo(127f, 920f, 103.5f, 896.5f); quadTo(80f, 873f, 80f, 840f)
                verticalLineTo(320f); horizontalLineTo(160f); verticalLineTo(840f)
                horizontalLineTo(640f); verticalLineTo(920f); horizontalLineTo(160f)
                close()
            },
            fill = androidx.compose.ui.graphics.SolidColor(Color.Black),
        )
    }.build()
}
