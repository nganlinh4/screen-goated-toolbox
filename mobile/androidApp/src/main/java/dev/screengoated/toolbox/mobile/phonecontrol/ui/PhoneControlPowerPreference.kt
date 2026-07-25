package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.content.Context
import android.content.SharedPreferences
import androidx.core.content.edit
import java.io.Closeable

internal enum class PhoneControlPowerChoice(
    val wireName: String,
    val elevatedProviderId: String?,
) {
    STANDARD("standard", null),
    SGT_ADB("sgt_adb", "sgt_adb_bridge"),
    SHIZUKU("shizuku", "shizuku_shell"),
    ROOT("root", "root_bridge"),
    ;

    fun enablesProvider(providerId: String): Boolean = elevatedProviderId == providerId
}

internal enum class PhoneControlPowerSelectionRoute {
    NONE,
    SETUP,
    RESUME_CAPTURE,
}

internal data class PhoneControlPowerChoicePresentation(
    val selected: Boolean,
    val recommended: Boolean,
)

internal fun phoneControlPowerChoicePresentation(
    choice: PhoneControlPowerChoice,
    selectedChoice: PhoneControlPowerChoice?,
) = PhoneControlPowerChoicePresentation(
    selected = choice == selectedChoice,
    recommended = choice == PhoneControlPowerChoice.SGT_ADB,
)

internal fun phoneControlPowerSelectionRoute(
    choice: PhoneControlPowerChoice,
    freshProjectionRequired: Boolean,
): PhoneControlPowerSelectionRoute = when {
    freshProjectionRequired -> PhoneControlPowerSelectionRoute.RESUME_CAPTURE
    choice.elevatedProviderId != null -> PhoneControlPowerSelectionRoute.SETUP
    else -> PhoneControlPowerSelectionRoute.NONE
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

    fun enablesProvider(context: Context, providerId: String): Boolean =
        current(context)?.enablesProvider(providerId) == true

    fun observe(
        context: Context,
        onChanged: (PhoneControlPowerChoice?) -> Unit,
    ): Closeable {
        val preferences = context.preferences()
        val listener = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
            if (key == KEY_CHOICE) onChanged(current(context))
        }
        preferences.registerOnSharedPreferenceChangeListener(listener)
        return Closeable {
            preferences.unregisterOnSharedPreferenceChangeListener(listener)
        }
    }

    private fun Context.preferences() = getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    private const val PREFERENCES = "phone_control_power"
    private const val KEY_CHOICE = "choice"
}
