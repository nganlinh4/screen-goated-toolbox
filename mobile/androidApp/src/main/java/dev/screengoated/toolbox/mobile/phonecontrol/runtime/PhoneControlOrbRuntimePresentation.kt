package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

internal data class PhoneControlOrbRuntimePresentation(
    val stateLabel: String,
    val iconOverride: String? = null,
)

internal val orbThinkingPresentation = PhoneControlOrbRuntimePresentation(
    GeneratedPhoneControlContract.ORB_STATE_THINKING,
)

internal val orbRespondingPresentation = PhoneControlOrbRuntimePresentation(
    GeneratedPhoneControlContract.ORB_STATE_RESPONDING,
)

internal val orbDonePresentation = PhoneControlOrbRuntimePresentation(
    GeneratedPhoneControlContract.ORB_STATE_DONE,
)

internal fun GeminiLiveFunctionCall.orbPresentation(): PhoneControlOrbRuntimePresentation {
    val state = GeneratedPhoneControlContract.orbStateForTool(name)
    val icon = if (state == GeneratedPhoneControlContract.ORB_STATE_SCROLL) {
        GeneratedPhoneControlContract.scrollIconForDirection(
            ((args as? JsonObject)?.get("direction") as? JsonPrimitive)?.contentOrNull,
        )
    } else {
        null
    }
    return PhoneControlOrbRuntimePresentation(state, icon)
}
