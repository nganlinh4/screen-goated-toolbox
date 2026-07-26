package dev.screengoated.toolbox.mobile.phonecontrol.tools

import java.security.MessageDigest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

internal data class PhoneControlFailureFingerprint(
    val turnId: Long,
    val tool: String,
    val argumentDigest: String,
    val observationIdentity: String,
)

internal class PhoneControlRepeatFailureGuard(
    private val retryLimit: Int = PHONE_CONTROL_EQUIVALENT_FAILURE_RETRY_LIMIT,
    private val maximumTrackedRequests: Int = 32,
) {
    private data class FailureRecord(
        val code: String,
        val count: Int,
    )

    private var activeTurnId: Long? = null
    private val failures = LinkedHashMap<PhoneControlFailureFingerprint, FailureRecord>()

    init {
        require(retryLimit > 0)
        require(maximumTrackedRequests > 0)
    }

    @Synchronized
    fun fingerprint(
        turnId: Long,
        tool: String,
        arguments: JsonObject,
        observationIdentity: String,
    ): PhoneControlFailureFingerprint {
        rotateTurn(turnId)
        return PhoneControlFailureFingerprint(
            turnId = turnId,
            tool = tool,
            argumentDigest = canonicalJson(arguments).sha256(),
            observationIdentity = observationIdentity,
        )
    }

    @Synchronized
    fun isBlocked(fingerprint: PhoneControlFailureFingerprint): Boolean {
        rotateTurn(fingerprint.turnId)
        return failures[fingerprint]?.count?.let { it >= retryLimit } == true
    }

    @Synchronized
    fun observe(
        fingerprint: PhoneControlFailureFingerprint,
        response: JsonObject,
    ) {
        rotateTurn(fingerprint.turnId)
        val code = response.repeatGuardString("code")
        val effect = response.repeatGuardString("effect_status")
        val reconciled = response.repeatGuardBoolean("state_reconciled")
        if (reconciled == true && code == SUCCESS_CODE) {
            failures.clear()
            return
        }
        if (code == null || code == SUCCESS_CODE || effect != PROVEN_NO_EFFECT) {
            failures.remove(fingerprint)
            return
        }
        val previous = failures[fingerprint]
        val count = if (previous?.code == code) previous.count + 1 else 1
        failures[fingerprint] = FailureRecord(code, count)
        while (failures.size > maximumTrackedRequests) {
            failures.remove(failures.keys.first())
        }
    }

    @Synchronized
    fun failureCode(fingerprint: PhoneControlFailureFingerprint): String? =
        failures[fingerprint]?.code

    private fun rotateTurn(turnId: Long) {
        if (activeTurnId == turnId) return
        activeTurnId = turnId
        failures.clear()
    }
}

private fun canonicalJson(element: JsonElement): String = when (element) {
    JsonNull -> "null"
    is JsonPrimitive -> element.toString()
    is JsonArray -> element.joinToString(prefix = "[", postfix = "]") { canonicalJson(it) }
    is JsonObject -> element.entries
        .sortedBy(Map.Entry<String, JsonElement>::key)
        .joinToString(prefix = "{", postfix = "}") { (key, value) ->
            JsonPrimitive(key).toString() + ":" + canonicalJson(value)
        }
}

private fun String.sha256(): String = MessageDigest.getInstance("SHA-256")
    .digest(toByteArray(Charsets.UTF_8))
    .joinToString("") { "%02x".format(it) }

private fun JsonObject.repeatGuardString(name: String): String? =
    (get(name) as? JsonPrimitive)?.contentOrNull

private fun JsonObject.repeatGuardBoolean(name: String): Boolean? =
    (get(name) as? JsonPrimitive)?.contentOrNull?.toBooleanStrictOrNull()

private const val SUCCESS_CODE = "ok"
private const val PROVEN_NO_EFFECT = "proven_no_effect"
internal const val PHONE_CONTROL_EQUIVALENT_FAILURE_RETRY_LIMIT = 2
