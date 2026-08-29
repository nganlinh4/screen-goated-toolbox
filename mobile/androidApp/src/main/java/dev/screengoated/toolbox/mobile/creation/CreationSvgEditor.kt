@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package dev.screengoated.toolbox.mobile.creation

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale
import dev.screengoated.toolbox.mobile.ui.i18n.CreationSvgLocale

internal class CreationSvgDocumentController {
    internal var document by mutableStateOf<NativeSvgDocument?>(null)
        private set
    internal var revision by mutableIntStateOf(0)
        private set
    internal var zoom by mutableFloatStateOf(1f)
        private set
    internal var pan by mutableStateOf(Offset.Zero)
        private set
    internal var selectedIndex by mutableStateOf<Int?>(null)
        private set

    private val undo = ArrayDeque<SvgEditDelta>()
    private val redo = ArrayDeque<SvgEditDelta>()
    internal val isEditable: Boolean get() = document?.editable == true

    internal fun attach(value: NativeSvgDocument) {
        if (document === value) return
        document = value
        selectedIndex = null
        undo.clear()
        redo.clear()
        fit()
    }

    internal fun transform(panChange: Offset, zoomChange: Float) {
        zoom = (zoom * zoomChange).coerceIn(0.25f, 8f)
        pan = if (zoom <= 1f) Offset.Zero else pan + panChange
    }

    internal fun select(index: Int?) {
        selectedIndex = index
        revision += 1
    }

    fun fit() {
        zoom = 1f
        pan = Offset.Zero
    }

    fun zoomIn() {
        zoom = (zoom * 1.2f).coerceAtMost(8f)
    }

    fun zoomOut() {
        zoom = (zoom / 1.2f).coerceAtLeast(0.25f)
        if (zoom <= 1f) pan = Offset.Zero
    }

    fun undo() {
        val value = document ?: return
        val delta = undo.removeLastOrNull() ?: return
        value.applyEdit(delta.index, delta.before)
        redo.addLast(delta)
        selectedIndex = delta.selectedBefore
        revision += 1
    }

    fun redo() {
        val value = document ?: return
        val delta = redo.removeLastOrNull() ?: return
        value.applyEdit(delta.index, delta.after)
        undo.addLast(delta)
        selectedIndex = delta.selectedAfter
        revision += 1
    }

    fun deleteSelected() = mutate { shape -> shape.deleted = true }
    fun setFill(value: String) = mutate { shape -> shape.fill = value }
    fun setStroke(value: String) = mutate { shape -> shape.stroke = value }
    suspend fun serialize(): String = document?.serialize().orEmpty()

    internal fun destroy() {
        document = null
        undo.clear()
        redo.clear()
    }

    private fun mutate(action: (NativeSvgShape) -> Unit) {
        val value = document ?: return
        val index = selectedIndex ?: return
        val shape = value.shapes.getOrNull(index) ?: return
        val before = shape.edit()
        action(shape)
        val after = shape.edit()
        if (before == after) return
        undo.addLast(SvgEditDelta(index, before, after, selectedIndex, selectedIndex))
        while (undo.size > MAXIMUM_UNDO_DELTAS) undo.removeFirst()
        redo.clear()
        revision += 1
    }

    private companion object {
        const val MAXIMUM_UNDO_DELTAS = 100
    }
}

internal data class SvgEditDelta(
    val index: Int,
    val before: SvgShapeEdit,
    val after: SvgShapeEdit,
    val selectedBefore: Int?,
    val selectedAfter: Int?,
)

@Composable
internal fun CreationSvgEditorControls(
    controller: CreationSvgDocumentController,
    common: CreationCommonLocale,
    strings: CreationSvgLocale,
    accent: Color,
    onSave: () -> Unit,
) {
    val swatches = listOf(
        "none" to Color.Transparent,
        "#111111" to Color(0xff111111),
        "#ffffff" to Color.White,
        "#1976d2" to Color(0xff1976d2),
        "#00a38c" to Color(0xff00a38c),
        "#e14d72" to Color(0xffe14d72),
        "#f4b400" to Color(0xfff4b400),
    )
    androidx.compose.foundation.layout.Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        FlowRow(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            ViewerIconButton(R.drawable.ms_open_in_full, strings.fit, controller::fit)
            ViewerIconButton(R.drawable.ms_remove, strings.zoomOut, controller::zoomOut)
            ViewerIconButton(R.drawable.ms_add, strings.zoomIn, controller::zoomIn)
            ViewerIconButton(R.drawable.ms_arrow_back, strings.undo, controller::undo)
            ViewerIconButton(R.drawable.ms_arrow_forward, strings.redo, controller::redo)
            ViewerIconButton(R.drawable.ms_delete, common.delete, controller::deleteSelected)
            FilledTonalButton(onClick = onSave) {
                Icon(painterResource(R.drawable.ms_check), null, Modifier.size(18.dp))
                androidx.compose.foundation.layout.Spacer(Modifier.size(6.dp))
                Text(strings.saveEdits)
            }
        }
        PaintSwatches(strings.fill, swatches, accent) { controller.setFill(it) }
        PaintSwatches(strings.stroke, swatches, accent) { controller.setStroke(it) }
    }
}

@Composable
private fun ViewerIconButton(icon: Int, label: String, action: () -> Unit) {
    IconButton(onClick = action, modifier = Modifier.size(40.dp)) {
        Icon(painterResource(icon), contentDescription = label)
    }
}

@Composable
private fun PaintSwatches(
    label: String,
    swatches: List<Pair<String, Color>>,
    accent: Color,
    onSelect: (String) -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(label, style = MaterialTheme.typography.labelMedium, modifier = Modifier.size(48.dp, 24.dp))
        FlowRow(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            swatches.forEach { (value, color) ->
                Box(
                    Modifier
                        .size(25.dp)
                        .background(
                            if (color == Color.Transparent) {
                                MaterialTheme.colorScheme.surface
                            } else {
                                color
                            },
                            CircleShape,
                        )
                        .border(
                            1.dp,
                            if (color == Color.Transparent) {
                                accent
                            } else {
                                MaterialTheme.colorScheme.outlineVariant
                            },
                            CircleShape,
                        )
                        .semantics { contentDescription = "$label $value" }
                        .clickable { onSelect(value) },
                )
            }
        }
    }
}
