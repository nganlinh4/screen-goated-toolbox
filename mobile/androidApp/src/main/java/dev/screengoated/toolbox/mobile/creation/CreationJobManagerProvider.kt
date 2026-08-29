package dev.screengoated.toolbox.mobile.creation

import android.content.Context

internal object CreationJobManagerProvider {
    @Volatile
    private var instance: CreationJobManager? = null

    fun get(context: Context): CreationJobManager = instance ?: synchronized(this) {
        instance ?: CreationJobManager(context.applicationContext).also { instance = it }
    }
}
