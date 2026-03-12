package dev.screengoated.toolbox.mobile.storage

import android.content.Intent
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager

class ProjectionConsentStore {
    @Volatile
    private var resultCode: Int? = null

    @Volatile
    private var dataIntent: Intent? = null

    fun update(resultCode: Int, data: Intent?) {
        this.resultCode = resultCode
        dataIntent = data?.let(::Intent)
    }

    fun clear() {
        resultCode = null
        dataIntent = null
    }

    fun hasConsent(): Boolean {
        return resultCode != null && dataIntent != null
    }

    fun createMediaProjection(manager: MediaProjectionManager): MediaProjection? {
        val safeResultCode = resultCode ?: return null
        val safeIntent = dataIntent?.let(::Intent) ?: return null
        return manager.getMediaProjection(safeResultCode, safeIntent)
    }
}

