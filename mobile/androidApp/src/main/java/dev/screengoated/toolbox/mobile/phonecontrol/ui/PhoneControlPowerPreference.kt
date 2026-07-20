package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.content.Context
import androidx.core.content.edit

internal enum class PhoneControlPowerChoice(val wireName: String) {
    STANDARD("standard"),
    SHIZUKU("shizuku"),
    ROOT("root"),
}

internal object PhoneControlPowerPreferences {
    fun current(context: Context): PhoneControlPowerChoice? = context.preferences()
        .getString(KEY_CHOICE, null)
        ?.let { value -> PhoneControlPowerChoice.entries.firstOrNull { it.wireName == value } }

    fun save(context: Context, choice: PhoneControlPowerChoice) {
        context.preferences().edit { putString(KEY_CHOICE, choice.wireName) }
    }

    fun clear(context: Context) {
        context.preferences().edit { remove(KEY_CHOICE) }
    }

    private fun Context.preferences() = getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    private const val PREFERENCES = "phone_control_power"
    private const val KEY_CHOICE = "choice"
}
