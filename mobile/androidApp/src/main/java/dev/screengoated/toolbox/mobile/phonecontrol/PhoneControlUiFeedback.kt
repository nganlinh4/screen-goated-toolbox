package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import android.widget.Toast
import androidx.annotation.StringRes
import dev.screengoated.toolbox.mobile.ui.i18n.uiLocalized

internal fun Context.phoneControlString(
    @StringRes resource: Int,
    vararg formatArguments: Any,
): String = uiLocalized().getString(resource, *formatArguments)

internal fun Context.showPhoneControlToast(
    @StringRes resource: Int,
    vararg formatArguments: Any,
) {
    val localized = uiLocalized()
    val message = if (formatArguments.isEmpty()) {
        localized.getText(resource)
    } else {
        localized.getString(resource, *formatArguments)
    }
    Toast.makeText(localized, message, Toast.LENGTH_SHORT).show()
}
