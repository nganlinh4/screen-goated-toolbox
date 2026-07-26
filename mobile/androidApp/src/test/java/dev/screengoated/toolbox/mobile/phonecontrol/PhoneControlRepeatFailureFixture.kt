package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlRepeatFailureGuard
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal class PhoneControlRepeatFailureFixture {
    private val guard = PhoneControlRepeatFailureGuard()

    var dispatchAllowed: Boolean? = null
        private set
    var resultCode: String? = null
        private set
    var effectMayHaveOccurred: Boolean? = null
        private set

    fun apply(raw: JsonObject) {
        val fingerprint = guard.fingerprint(
            turnId = FIXTURE_TURN_ID,
            tool = raw.fixtureString("tool"),
            arguments = buildJsonObject {
                put("requestFingerprint", raw.fixtureString("requestFingerprint"))
            },
            observationIdentity = FIXTURE_OBSERVATION,
        )
        when (raw.fixtureString("type")) {
            "toolFailure" -> guard.observe(
                fingerprint,
                buildJsonObject {
                    put("code", raw.fixtureString("code"))
                    put("effect_status", "proven_no_effect")
                },
            )
            "toolCall" -> {
                dispatchAllowed = !guard.isBlocked(fingerprint)
                if (dispatchAllowed == false) {
                    resultCode = "repeated_failure"
                    effectMayHaveOccurred = false
                }
            }
        }
    }
}

internal val PHONE_CONTROL_EXTERNAL_FIXTURE_EVENT_TYPES = setOf(
    "toolRequested",
    "providerState",
    "browserNavigationRequested",
    "customTabOpened",
    "semanticActionRequested",
    "cdpTargetProbe",
    "providerRoute",
    "rejectionFlood",
    "toolFrameOverflow",
    "queuedControlPayload",
    "sessionReconnect",
    "freshProtocolSession",
    "ownedEffectBoundary",
    "platformDispatchAttempt",
    "platformEffectAccepted",
    "providerTerminalCallback",
    "ambientScreenFrame",
    "microphoneAudio",
    "toolFailure",
)

private fun JsonObject.fixtureString(name: String): String =
    getValue(name).jsonPrimitive.content

private const val FIXTURE_TURN_ID = 1L
private const val FIXTURE_OBSERVATION = "generation=1;visual_revision=1"
