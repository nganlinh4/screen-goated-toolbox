package dev.screengoated.toolbox.mobile.preset

private const val RATE_LIMIT_DEFAULT_MILLIS = 5L * 60L * 1_000L
private const val TIMEOUT_OPEN_MILLIS = 30L * 60L * 1_000L
private const val UNAVAILABLE_OPEN_MILLIS = 6L * 60L * 60L * 1_000L
private const val BILLING_OPEN_MILLIS = 6L * 60L * 60L * 1_000L
private const val MINIMUM_REPORTED_MILLIS = 5_000L
private const val MAXIMUM_REPORTED_MILLIS = 6L * 60L * 60L * 1_000L
private const val TIMEOUT_FAILURE_THRESHOLD = 2

internal enum class PresetCircuitKind(val reason: String, val durationMillis: Long) {
    RATE_LIMIT("MODEL_RATE_LIMIT_COOLDOWN", RATE_LIMIT_DEFAULT_MILLIS),
    TIMEOUT("MODEL_TIMEOUT_COOLDOWN", TIMEOUT_OPEN_MILLIS),
    UNAVAILABLE("MODEL_UNAVAILABLE_COOLDOWN", UNAVAILABLE_OPEN_MILLIS),
    BILLING("MODEL_BILLING_COOLDOWN", BILLING_OPEN_MILLIS),
}

private sealed interface PresetCircuitState {
    data class Monitoring(val timeoutFailures: Int) : PresetCircuitState
    data class Open(val kind: PresetCircuitKind, val untilMillis: Long) : PresetCircuitState
    data class HalfOpen(val kind: PresetCircuitKind) : PresetCircuitState
}

private val presetCircuitLock = Any()
private val presetCircuits = mutableMapOf<String, PresetCircuitState>()
private fun monotonicMillis(): Long = System.nanoTime() / 1_000_000L

internal fun recordPresetModelFailure(modelId: String, error: String) {
    recordPresetModelFailureAt(modelId, error, monotonicMillis())
}

internal fun recordPresetModelSuccess(modelId: String) {
    synchronized(presetCircuitLock) { presetCircuits.remove(modelId) }
}

internal fun releasePresetModelProbe(modelId: String) {
    releasePresetModelProbeAt(modelId, monotonicMillis())
}

internal fun releasePresetModelProbeAt(modelId: String, nowMillis: Long) {
    synchronized(presetCircuitLock) {
        val state = presetCircuits[modelId]
        if (state is PresetCircuitState.HalfOpen) {
            presetCircuits[modelId] = PresetCircuitState.Open(state.kind, nowMillis)
        }
    }
}

internal fun claimPresetModelAttempt(modelId: String): String? =
    claimPresetModelAttemptAt(modelId, monotonicMillis())

internal fun presetModelCircuitSkipReason(modelId: String): String? =
    presetModelCircuitSkipReasonAt(modelId, monotonicMillis())

internal fun recordPresetModelFailureAt(modelId: String, error: String, nowMillis: Long) {
    val rateLimited = isRateLimitError(error)
    val timedOut = isTimeoutError(error)
    val unavailable = isUnavailableModelError(error)
    val billing = isBillingError(error)
    val reported = if (rateLimited) reportedCooldownMillis(error) else null
    fun duration(kind: PresetCircuitKind): Long =
        if (kind == PresetCircuitKind.RATE_LIMIT) reported ?: kind.durationMillis
        else kind.durationMillis

    synchronized(presetCircuitLock) {
        var state = presetCircuits[modelId] ?: PresetCircuitState.Monitoring(0)
        if (state is PresetCircuitState.Open && state.untilMillis <= nowMillis) {
            state = PresetCircuitState.Monitoring(0)
        }
        val next = when {
            state is PresetCircuitState.Open && state.untilMillis > nowMillis -> state
            state is PresetCircuitState.HalfOpen -> {
                val kind = when {
                    billing -> PresetCircuitKind.BILLING
                    rateLimited -> PresetCircuitKind.RATE_LIMIT
                    timedOut -> PresetCircuitKind.TIMEOUT
                    unavailable -> PresetCircuitKind.UNAVAILABLE
                    else -> state.kind
                }
                PresetCircuitState.Open(kind, nowMillis + duration(kind))
            }
            billing -> PresetCircuitState.Open(
                PresetCircuitKind.BILLING,
                nowMillis + BILLING_OPEN_MILLIS,
            )
            rateLimited -> PresetCircuitState.Open(
                PresetCircuitKind.RATE_LIMIT,
                nowMillis + duration(PresetCircuitKind.RATE_LIMIT),
            )
            unavailable -> PresetCircuitState.Open(
                PresetCircuitKind.UNAVAILABLE,
                nowMillis + UNAVAILABLE_OPEN_MILLIS,
            )
            timedOut && state is PresetCircuitState.Monitoring -> {
                val failures = state.timeoutFailures + 1
                if (failures >= TIMEOUT_FAILURE_THRESHOLD) {
                    PresetCircuitState.Open(
                        PresetCircuitKind.TIMEOUT,
                        nowMillis + TIMEOUT_OPEN_MILLIS,
                    )
                } else {
                    PresetCircuitState.Monitoring(failures)
                }
            }
            else -> state
        }
        if (next is PresetCircuitState.Monitoring && next.timeoutFailures == 0 &&
            !rateLimited && !timedOut && !unavailable && !billing
        ) {
            presetCircuits.remove(modelId)
        } else {
            presetCircuits[modelId] = next
        }
    }
}

internal fun claimPresetModelAttemptAt(modelId: String, nowMillis: Long): String? =
    synchronized(presetCircuitLock) {
        when (val state = presetCircuits[modelId]) {
            null, is PresetCircuitState.Monitoring -> null
            is PresetCircuitState.Open -> if (state.untilMillis > nowMillis) {
                circuitReason(state.kind, modelId, state.untilMillis - nowMillis)
            } else {
                presetCircuits[modelId] = PresetCircuitState.HalfOpen(state.kind)
                null
            }
            is PresetCircuitState.HalfOpen -> "MODEL_COOLDOWN_PROBE_IN_FLIGHT:$modelId"
        }
    }

internal fun presetModelCircuitSkipReasonAt(modelId: String, nowMillis: Long): String? =
    synchronized(presetCircuitLock) {
        when (val state = presetCircuits[modelId]) {
            null, is PresetCircuitState.Monitoring -> null
            is PresetCircuitState.Open -> state.takeIf { it.untilMillis > nowMillis }
                ?.let { circuitReason(it.kind, modelId, it.untilMillis - nowMillis) }
            is PresetCircuitState.HalfOpen -> "MODEL_COOLDOWN_PROBE_IN_FLIGHT:$modelId"
        }
    }

internal fun reportedCooldownMillis(error: String): Long? {
    val lower = error.lowercase()
    val tail = listOf("try again in ", "retry in ", "retry after ", "retry-after: ")
        .firstNotNullOfOrNull { marker -> lower.substringAfter(marker, "").takeIf(String::isNotEmpty) }
        ?: return null
    val seconds = parseDurationSeconds(tail.trim()) ?: return null
    return (seconds * 1_000.0).toLong().coerceIn(
        MINIMUM_REPORTED_MILLIS,
        MAXIMUM_REPORTED_MILLIS,
    )
}

internal fun parseDurationSeconds(text: String): Double? {
    var total = 0.0
    val number = StringBuilder()
    var matched = false
    var index = 0
    while (index < text.length) {
        val character = text[index]
        when {
            character.isDigit() || character == '.' -> number.append(character)
            character in "hms" && number.isNotEmpty() -> {
                val value = number.toString().toDoubleOrNull() ?: return null
                val millis = character == 'm' && text.getOrNull(index + 1) == 's'
                total += when {
                    character == 'h' -> value * 3_600.0
                    character == 'm' && !millis -> value * 60.0
                    character == 'm' -> value / 1_000.0
                    else -> value
                }
                if (millis) index += 1
                number.clear()
                matched = true
            }
            else -> break
        }
        index += 1
    }
    if (number.isNotEmpty() && !matched) {
        total = number.toString().toDoubleOrNull() ?: return null
        matched = true
    }
    return total.takeIf { matched && it > 0.0 }
}

private fun isRateLimitError(error: String): Boolean {
    val lower = error.lowercase()
    return "http 429" in lower || "status code 429" in lower || "request failed with 429" in lower ||
        "rate limit" in lower || "too many requests" in lower || "quota exceeded" in lower
}

private fun isTimeoutError(error: String): Boolean {
    val lower = error.lowercase()
    return "timeout" in lower || "timed out" in lower || "deadline exceeded" in lower
}

private fun isBillingError(error: String): Boolean {
    val lower = error.lowercase()
    return listOf(
        "payment required",
        "insufficient credit",
        "insufficient_quota",
        "insufficient funds",
        "out of credits",
        "credit balance is too low",
    ).any { it in lower }
}

private fun isUnavailableModelError(error: String): Boolean {
    val lower = error.lowercase()
    val status404 = listOf("http 404", "status code 404", "error 404").any { it in lower }
    return status404 && ("model" in lower || "deployment" in lower) && listOf(
        "unavailable",
        "archived",
        "not found",
        "no such model",
        "does not exist",
        "doesn't exist",
        "decommissioned",
        "deprecated",
        "has been removed",
        "was removed",
    ).any { it in lower }
}

private fun circuitReason(kind: PresetCircuitKind, modelId: String, remainingMillis: Long): String =
    "${kind.reason}:$modelId:${((remainingMillis + 999L) / 1_000L).coerceAtLeast(1L)}s"

internal fun clearPresetModelCircuitsForTest() {
    synchronized(presetCircuitLock) { presetCircuits.clear() }
}
