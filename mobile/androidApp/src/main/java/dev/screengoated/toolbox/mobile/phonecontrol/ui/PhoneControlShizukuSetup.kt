package dev.screengoated.toolbox.mobile.phonecontrol.ui

import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuBridgeCondition
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuBridgeProbe

internal enum class PhoneControlShizukuSetupAction(val wireName: String) {
    COMPLETE("complete"),
    REQUEST_PERMISSION("request_permission"),
    OPEN_MANAGER("open_manager"),
    OPEN_STORE("open_store"),
}

internal data class PhoneControlShizukuSetupAttempt(
    val condition: ShizukuBridgeCondition,
    val action: PhoneControlShizukuSetupAction,
)

internal fun nextPhoneControlShizukuSetupAction(
    probe: ShizukuBridgeProbe,
): PhoneControlShizukuSetupAction = when (probe.condition) {
    ShizukuBridgeCondition.READY -> PhoneControlShizukuSetupAction.COMPLETE
    ShizukuBridgeCondition.PERMISSION_REQUESTABLE ->
        PhoneControlShizukuSetupAction.REQUEST_PERMISSION
    ShizukuBridgeCondition.SERVICE_STOPPED,
    ShizukuBridgeCondition.PERMISSION_REVOKED,
    -> PhoneControlShizukuSetupAction.OPEN_MANAGER
    ShizukuBridgeCondition.PACKAGE_MISSING,
    ShizukuBridgeCondition.API_UNSUPPORTED,
    -> PhoneControlShizukuSetupAction.OPEN_STORE
}
