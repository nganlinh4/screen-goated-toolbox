package dev.screengoated.toolbox.mobile.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.storage.LegacyOfflineModels
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private sealed interface LegacyCleanupState {
    data object Absent : LegacyCleanupState
    data class Present(val bytes: Long, val failed: Boolean = false) : LegacyCleanupState
    data class Removing(val bytes: Long) : LegacyCleanupState
}

/** One row offering to remove files that removed offline engines left behind. */
@Composable
internal fun LegacyOfflineModelsCard(
    locale: MobileLocaleText,
    onHelp: (Pair<String, String>) -> Unit,
) {
    val context = LocalContext.current
    val legacy = remember {
        LegacyOfflineModels(
            filesDir = context.filesDir,
            cacheDir = context.cacheDir,
            deletePreferences = { name -> context.deleteSharedPreferences(name) },
        )
    }
    val scope = rememberCoroutineScope()
    var state by remember { mutableStateOf<LegacyCleanupState>(LegacyCleanupState.Absent) }
    LaunchedEffect(legacy) {
        state = withContext(Dispatchers.IO) { legacy.presentBytes() }
            ?.let { LegacyCleanupState.Present(it) }
            ?: LegacyCleanupState.Absent
    }
    val current = state
    if (current == LegacyCleanupState.Absent) return

    val name = locale.downloader.legacyOfflineModels
    ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.error) {
        DownloadedToolRow(
            name = name,
            icon = R.drawable.ms_delete,
            statusText = when (current) {
                is LegacyCleanupState.Present -> if (current.failed) {
                    "${locale.downloader.legacyOfflineModelsRemoveFailed} (${formatMb(current.bytes)} MB)"
                } else {
                    "${formatMb(current.bytes)} MB"
                }
                is LegacyCleanupState.Removing -> "${formatMb(current.bytes)} MB"
                LegacyCleanupState.Absent -> ""
            },
            statusColor = if ((current as? LegacyCleanupState.Present)?.failed == true) {
                MaterialTheme.colorScheme.error
            } else {
                MaterialTheme.colorScheme.onSurfaceVariant
            },
            accent = MaterialTheme.colorScheme.error,
            onHelpClick = { onHelp(name to locale.downloader.legacyOfflineModelsDesc) },
            progressFraction = if (current is LegacyCleanupState.Removing) 0f else null,
            action = (current as? LegacyCleanupState.Present)?.let { present ->
                ToolAction(
                    text = locale.downloader.legacyOfflineModelsRemove,
                    role = ToolActionRole.DESTRUCTIVE,
                    onClick = {
                        state = LegacyCleanupState.Removing(present.bytes)
                        scope.launch {
                            state = withContext(Dispatchers.IO) {
                                val removed = legacy.remove()
                                legacy.presentBytes()
                                    ?.let { LegacyCleanupState.Present(it, failed = !removed) }
                                    ?: LegacyCleanupState.Absent
                            }
                        }
                    },
                )
            },
        )
    }
}
