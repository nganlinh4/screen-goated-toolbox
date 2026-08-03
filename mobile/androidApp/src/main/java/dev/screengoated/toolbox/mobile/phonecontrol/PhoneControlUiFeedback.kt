package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import android.widget.Toast
import androidx.annotation.StringRes
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeCode
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

internal fun Context.phoneControlRuntimeMessage(code: PhoneControlRuntimeCode): String =
    phoneControlString(
        when (code) {
            PhoneControlRuntimeCode.STOPPED -> R.string.phone_control_status_stopped
            PhoneControlRuntimeCode.STARTING -> R.string.phone_control_status_starting
            PhoneControlRuntimeCode.CONNECTING -> R.string.phone_control_status_connecting
            PhoneControlRuntimeCode.READY -> R.string.phone_control_status_ready
            PhoneControlRuntimeCode.WORKING -> R.string.phone_control_status_working
            PhoneControlRuntimeCode.FINALIZING -> R.string.phone_control_status_finalizing
            PhoneControlRuntimeCode.RECONNECTING -> R.string.phone_control_status_reconnecting
            PhoneControlRuntimeCode.ACCESSIBILITY_UNAVAILABLE ->
                R.string.phone_control_status_accessibility_unavailable
            PhoneControlRuntimeCode.SCREEN_CAPTURE_FAILED ->
                R.string.phone_control_status_capture_failed
            PhoneControlRuntimeCode.SCREEN_SHARE_REQUIRED ->
                R.string.phone_control_status_projection_required
            PhoneControlRuntimeCode.API_KEY_REQUIRED ->
                R.string.phone_control_status_api_key_required
            PhoneControlRuntimeCode.CONFIGURATION_FAILED ->
                R.string.phone_control_status_configuration_failed
            PhoneControlRuntimeCode.MICROPHONE_FAILED ->
                R.string.phone_control_status_microphone_failed
            PhoneControlRuntimeCode.TRANSPORT_FAILED ->
                R.string.phone_control_status_transport_failed
            PhoneControlRuntimeCode.RUNTIME_FAILED -> R.string.phone_control_status_runtime_failed
        },
    )
