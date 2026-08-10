package dev.screengoated.toolbox.mobile.service.nativelibs

import android.content.Context

/** Persists removals that cannot finish until loaded native code leaves the process. */
internal class NativeRuntimeRemovalStore(context: Context) {
    private val preferences = context.applicationContext.getSharedPreferences(
        PREFERENCES_NAME,
        Context.MODE_PRIVATE,
    )

    fun isPending(engineName: String): Boolean =
        preferences.getBoolean(key(engineName), false)

    fun setPending(engineName: String, pending: Boolean) {
        preferences.edit().run {
            if (pending) putBoolean(key(engineName), true) else remove(key(engineName))
        }.apply()
    }

    private fun key(engineName: String): String = "pending_${engineName.lowercase()}"

    private companion object {
        const val PREFERENCES_NAME = "downloaded_native_runtime_lifecycle"
    }
}
