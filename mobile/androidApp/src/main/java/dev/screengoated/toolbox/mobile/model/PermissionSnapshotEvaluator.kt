package dev.screengoated.toolbox.mobile.model

import android.Manifest
import android.content.Context
import android.os.Build
import android.provider.Settings
import androidx.core.content.ContextCompat
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.PermissionSnapshot
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore

class PermissionSnapshotEvaluator(
    private val projectionConsentStore: ProjectionConsentStore,
) {
    fun runtimePermissions(): Array<String> {
        return buildList {
            add(Manifest.permission.RECORD_AUDIO)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                add(Manifest.permission.POST_NOTIFICATIONS)
            }
        }.toTypedArray()
    }

    fun evaluate(
        context: Context,
        config: LiveSessionConfig,
        overlaySupported: Boolean,
    ): PermissionSnapshot {
        val recordAudioGranted = ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.RECORD_AUDIO,
        ) == android.content.pm.PackageManager.PERMISSION_GRANTED

        val notificationsGranted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == android.content.pm.PackageManager.PERMISSION_GRANTED
        } else {
            true
        }

        val overlayGranted = when {
            !overlaySupported -> true
            config.displayMode != DisplayMode.OVERLAY -> true
            else -> Settings.canDrawOverlays(context)
        }

        val mediaProjectionGranted = when (config.sourceMode) {
            SourceMode.MIC -> true
            SourceMode.DEVICE -> projectionConsentStore.hasConsent()
        }

        return PermissionSnapshot(
            recordAudioGranted = recordAudioGranted,
            notificationsGranted = notificationsGranted,
            overlayGranted = overlayGranted,
            mediaProjectionGranted = mediaProjectionGranted,
        )
    }
}
