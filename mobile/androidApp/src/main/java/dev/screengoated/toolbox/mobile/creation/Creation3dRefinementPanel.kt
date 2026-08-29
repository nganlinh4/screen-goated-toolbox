package dev.screengoated.toolbox.mobile.creation

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.UtilityExpressiveCard
import dev.screengoated.toolbox.mobile.ui.UtilityHeaderRow
import dev.screengoated.toolbox.mobile.ui.i18n.Creation3dRefinementLocale

@Composable
internal fun Creation3dRefinementPanel(
    status: CreationJobStatus,
    strings: Creation3dRefinementLocale,
    accent: Color,
    onRefine: (String, Int?, String?) -> Unit,
) {
    val actions = status.availableActions.toSet()
    val supported = (status.supportedActions ?: status.availableActions).toSet()
    if (supported.isEmpty()) return
    var targetFaces by remember(status.dispatchId) {
        mutableIntStateOf((status.faces ?: 5_000).toInt().coerceIn(100, 20_000))
    }
    Column(
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier.testTag("creation-refinement-panel"),
    ) {
        if ("separate_parts" in supported) {
            UtilityExpressiveCard(accent = accent) {
                UtilityHeaderRow(R.drawable.ms_layers, strings.separation, accent)
                ActionRow(
                    listOf("separate_detailed" to strings.detailed),
                    actions,
                    supported,
                    "separate_parts",
                ) { onRefine(it, null, null) }
            }
        }
        if (supported.any { it.startsWith("optimize_") }) {
            UtilityExpressiveCard(accent = accent) {
                UtilityHeaderRow(R.drawable.ms_tune, strings.optimize, accent)
                Text("${strings.targetFaces}: $targetFaces", style = MaterialTheme.typography.bodyMedium)
                Slider(
                    value = targetFaces.toFloat(),
                    onValueChange = { targetFaces = (it / 100f).toInt() * 100 },
                    valueRange = 100f..20_000f,
                    enabled = actions.any { it.startsWith("optimize_") },
                    modifier = Modifier.fillMaxWidth(),
                )
                ActionRow(
                    listOf(
                        "optimize_triangle" to strings.triangle,
                        "optimize_quad" to strings.quad,
                    ),
                    actions,
                    supported,
                ) { onRefine(it, targetFaces, null) }
            }
        }
        if (supported.any(ENRICHMENT_CAPABILITIES::contains)) {
            UtilityExpressiveCard(accent = accent) {
                UtilityHeaderRow(R.drawable.ms_auto_awesome, strings.title, accent)
                ActionRow(
                    listOf(
                        "materials" to strings.materials,
                        "pbr" to strings.pbr,
                        "rig" to strings.rig,
                    ),
                    actions,
                    supported,
                ) { onRefine(it, null, null) }
                if ("animate" in supported) {
                    Text(strings.animation, style = MaterialTheme.typography.labelLarge)
                    ActionRow(
                        listOf(
                            "animate_idle" to strings.idle,
                            "animate_walk" to strings.walk,
                            "animate_run" to strings.run,
                        ),
                        actions,
                        supported,
                        "animate",
                    ) { action -> onRefine(action, null, action.removePrefix("animate_")) }
                    ActionRow(
                        listOf("animate_jump" to strings.jump, "animate_wave" to strings.wave),
                        actions,
                        supported,
                        "animate",
                    ) { action -> onRefine(action, null, action.removePrefix("animate_")) }
                }
            }
        }
    }
}

@Composable
private fun ActionRow(
    options: List<Pair<String, String>>,
    actions: Set<String>,
    supported: Set<String>,
    sharedCapability: String? = null,
    onAction: (String) -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        options.filter { (action) ->
            (sharedCapability ?: CreationContract.refinementCapability(action)) in supported
        }.forEach { (action, label) ->
            TextButton(
                onClick = { onAction(action) },
                enabled = (sharedCapability ?: CreationContract.refinementCapability(action)) in actions,
                modifier = Modifier.weight(1f).testTag("creation-refine-$action"),
            ) { Text(label, maxLines = 1) }
        }
    }
}

private val ENRICHMENT_CAPABILITIES = setOf("add_materials", "generate_pbr", "rig", "animate")
