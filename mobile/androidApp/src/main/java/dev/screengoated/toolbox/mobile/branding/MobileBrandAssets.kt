package dev.screengoated.toolbox.mobile.branding

import android.content.res.Configuration
import androidx.annotation.DrawableRes
import dev.screengoated.toolbox.mobile.R

object MobileBrandAssets {
    const val WINDOWS_DARK_ICON_SOURCE = "assets/app-icon-small.png"
    const val WINDOWS_LIGHT_ICON_SOURCE = "assets/app-icon-small-light.png"

    @DrawableRes
    fun appIcon(isDarkSurface: Boolean): Int {
        return if (isDarkSurface) {
            R.drawable.sgt_brand_dark
        } else {
            R.drawable.sgt_brand_light
        }
    }

    @DrawableRes
    fun notificationLargeIcon(configuration: Configuration): Int {
        return appIcon(isDarkTheme(configuration))
    }

    fun isDarkTheme(configuration: Configuration): Boolean {
        return (configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
    }
}
