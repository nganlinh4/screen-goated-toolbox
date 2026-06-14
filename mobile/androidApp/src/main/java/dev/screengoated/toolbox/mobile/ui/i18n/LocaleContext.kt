package dev.screengoated.toolbox.mobile.ui.i18n

import android.content.Context
import android.content.res.Configuration
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import java.util.Locale

/**
 * Returns a [Context] whose resources resolve string resources for the current in-app UI
 * language (Settings → Language), so `getString(...)` follows the in-app language instead of
 * the device/system locale.
 *
 * The in-app language is the canonical UI-language source on Windows, where a single setting
 * drives all user-facing text; this keeps Android OS-surfaced strings (notification channels,
 * foreground-service notifications, toasts, permission prompts) consistent with it. Without
 * this, a user who picks Korean/Vietnamese in-app on an English device would still see English
 * notifications, defeating the localized `values-ko`/`values-vi` resources.
 *
 * Falls back to the receiver unchanged when the application singleton is unavailable
 * (e.g. unit tests), which resolves to the default (English) resources.
 */
fun Context.uiLocalized(): Context {
    val app = applicationContext as? SgtMobileApplication ?: return this
    val tag = app.appContainer.repository.currentUiPreferences().uiLanguage.ifBlank { "en" }
    val config = Configuration(resources.configuration)
    config.setLocale(Locale.forLanguageTag(tag))
    return createConfigurationContext(config)
}
