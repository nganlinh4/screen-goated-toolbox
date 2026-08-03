package dev.screengoated.toolbox.mobile.phonecontrol

internal enum class PhoneControlAuthorityAutomationDisposition {
    NONE,
    SUBMIT,
    COALESCE,
    BLOCKED,
}

internal data class PhoneControlAuthorityAutomationOwnership(
    val goalId: Long,
    val providerId: String,
    val checkpointMonitoring: Boolean,
)

internal fun phoneControlAuthorityAutomationDisposition(
    automationRequested: Boolean,
    requestedProvider: String,
    activeGoalId: Long?,
    activeProvider: String?,
): PhoneControlAuthorityAutomationDisposition = when {
    !automationRequested -> PhoneControlAuthorityAutomationDisposition.NONE
    activeGoalId == null -> PhoneControlAuthorityAutomationDisposition.SUBMIT
    activeProvider == requestedProvider -> PhoneControlAuthorityAutomationDisposition.COALESCE
    else -> PhoneControlAuthorityAutomationDisposition.BLOCKED
}

internal class PhoneControlAuthorityAutomationOwner {
    private var ownership: PhoneControlAuthorityAutomationOwnership? = null

    fun disposition(
        automationRequested: Boolean,
        requestedProvider: String,
    ): PhoneControlAuthorityAutomationDisposition = phoneControlAuthorityAutomationDisposition(
        automationRequested = automationRequested,
        requestedProvider = requestedProvider,
        activeGoalId = ownership?.goalId,
        activeProvider = ownership?.providerId,
    )

    fun begin(
        goalId: Long,
        providerId: String,
        checkpointMonitoring: Boolean,
    ): PhoneControlAuthorityAutomationOwnership {
        require(goalId > 0L)
        require(providerId.isNotBlank())
        check(ownership == null) { "authority automation already has an owner" }
        return PhoneControlAuthorityAutomationOwnership(
            goalId = goalId,
            providerId = providerId,
            checkpointMonitoring = checkpointMonitoring,
        ).also { ownership = it }
    }

    fun coalesce(
        providerId: String,
        checkpointMonitoring: Boolean,
    ): PhoneControlAuthorityAutomationOwnership? {
        val active = ownership?.takeIf { it.providerId == providerId } ?: return null
        return active.copy(
            checkpointMonitoring = active.checkpointMonitoring || checkpointMonitoring,
        ).also { ownership = it }
    }

    fun complete(goalId: Long): PhoneControlAuthorityAutomationOwnership? {
        val active = ownership?.takeIf { it.goalId == goalId } ?: return null
        ownership = null
        return active
    }

    fun clear(): PhoneControlAuthorityAutomationOwnership? {
        val active = ownership
        ownership = null
        return active
    }
}
