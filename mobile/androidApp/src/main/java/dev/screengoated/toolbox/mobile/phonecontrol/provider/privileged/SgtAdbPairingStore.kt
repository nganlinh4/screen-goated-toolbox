package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context

internal object SgtAdbPairingStore {
    fun isPaired(context: Context): Boolean =
        preferences(context).getBoolean(KEY_PAIRED, false) &&
            deviceIdentity(context) != null

    fun deviceIdentity(context: Context): String? =
        preferences(context)
            .getString(KEY_DEVICE_IDENTITY, null)
            ?.takeIf(::isSgtAdbDeviceIdentity)

    fun record(context: Context, deviceIdentity: String): Boolean {
        if (!isSgtAdbDeviceIdentity(deviceIdentity)) return false
        return preferences(context)
            .edit()
            .putString(KEY_DEVICE_IDENTITY, deviceIdentity)
            .putBoolean(KEY_PAIRED, true)
            .commit()
    }

    fun clear(context: Context): Boolean =
        preferences(context)
            .edit()
            .remove(KEY_DEVICE_IDENTITY)
            .putBoolean(KEY_PAIRED, false)
            .commit()

    private fun preferences(context: Context) =
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    private const val PREFERENCES = "phone_control_sgt_adb"
    private const val KEY_PAIRED = "paired"
    private const val KEY_DEVICE_IDENTITY = "device_identity"
}
