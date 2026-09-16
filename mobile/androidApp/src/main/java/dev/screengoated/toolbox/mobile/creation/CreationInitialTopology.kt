package dev.screengoated.toolbox.mobile.creation

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.ui.i18n.Creation3dRefinementLocale

@Composable
internal fun CreationInitialTopology(value: String, strings: Creation3dRefinementLocale, enabled: Boolean, select: (String) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        listOf("triangle" to strings.triangle, "quad" to strings.quad).forEach { (key, label) ->
            FilterChip(selected = value == key, onClick = { select(key) }, enabled = enabled,
                label = { Text(label) }, modifier = Modifier.testTag("creation-initial-topology-$key"))
        }
    }
}
