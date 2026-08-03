package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointReadiness
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityStructuralChangeBus
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntime
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalCompletion
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import java.io.Closeable

internal class PhoneControlProtectedCheckpointHandoff(
    private val context: Context,
    private val setup: PhoneControlProtectedSetupCoordinator,
    private val checkpoint: PhoneControlProtectedCheckpointController,
    private val runtime: () -> PhoneControlRuntime?,
    private val projectionActive: () -> Boolean,
    private val releaseProjection: () -> Unit,
    private val continueNavigation: (String, Long, String) -> Long?,
    private val navigationExhausted: (String, String) -> Unit,
) : Closeable {
    private val handler = Handler(Looper.getMainLooper())
    private var providerId: String? = null
    private var goalId: Long? = null
    private var goalOutcome: PhoneControlUiGoalOutcome? = null
    private var token: PhoneControlProtectedCheckpointToken? = null
    private var navigationDeadlineMs = 0L
    private var postGoalDeadlineMs = 0L
    private var boundaryDeadlineMs = 0L
    private var navigationAttempt = 0
    private var lastReadiness = "checkpoint_not_ready"
    private var structuralObserver: Closeable? = null

    private val readinessCheck = object : Runnable {
        override fun run() {
            val provider = providerId ?: return
            if (!requestStillValid(provider)) {
                cancelMonitor("state_changed")
                return
            }
            if (token == null) {
                when (val readiness = setup.checkpointReadiness(provider)) {
                    PhoneControlProtectedCheckpointReadiness.Ready -> seal(provider)
                    is PhoneControlProtectedCheckpointReadiness.NotReady -> {
                        lastReadiness = readiness.code
                    }
                }
            }
            if (token != null && goalOutcome != null) {
                startLocalAdapter(provider)
                return
            }
            val now = SystemClock.elapsedRealtime()
            if (token != null) {
                if (now >= boundaryDeadlineMs) {
                    PhoneControlLog.w(
                        TAG,
                        "protected_checkpoint_boundary accepted=false reason=owned_action_pending",
                    )
                    boundaryDeadlineMs = now + BOUNDARY_WARNING_MS
                }
                schedule(FALLBACK_CHECK_MS)
                return
            }
            when (
                phoneControlProtectedNavigationDecision(
                    goalFinished = goalOutcome != null,
                    postGoalSettled = postGoalDeadlineMs > 0L && now >= postGoalDeadlineMs,
                    attempt = navigationAttempt,
                    maximumAttempts = MAX_NAVIGATION_ATTEMPTS,
                    deadlineReached = now >= navigationDeadlineMs,
                )
            ) {
                PhoneControlProtectedNavigationDecision.WAIT -> Unit
                PhoneControlProtectedNavigationDecision.RETRY -> {
                    if (!continueFromFreshEvidence(provider)) return
                }
                PhoneControlProtectedNavigationDecision.EXHAUSTED -> {
                    val reason = if (now >= navigationDeadlineMs) {
                        "navigation_deadline"
                    } else {
                        "continuation_budget_exhausted"
                    }
                    failNavigation(provider, reason)
                    return
                }
            }
            schedule(FALLBACK_CHECK_MS)
        }
    }

    fun arm(provider: String, navigationGoalId: Long): Boolean {
        if (provider.isBlank() || navigationGoalId <= 0L) return false
        if (providerId == provider && goalId == navigationGoalId) return true
        val candidate = runtime()
        val policy = setup.capturePolicy(provider)
        if (candidate == null || !projectionActive() || checkpoint.active || policy == null) {
            PhoneControlLog.w(
                TAG,
                "protected_checkpoint_monitor accepted=false reason=state_or_adapter",
            )
            return false
        }
        cancelMonitor("replaced")
        providerId = provider
        goalId = navigationGoalId
        navigationDeadlineMs = SystemClock.elapsedRealtime() + NAVIGATION_TIMEOUT_MS
        navigationAttempt = 1
        structuralObserver = AccessibilityStructuralChangeBus.observe { schedule(EVENT_SETTLE_MS) }
        PhoneControlLog.i(
            TAG,
            "protected_checkpoint_monitor accepted=true provider=$provider goal_id=$navigationGoalId",
        )
        schedule(0L)
        return true
    }

    fun navigationFinished(
        provider: String,
        completion: PhoneControlUiGoalCompletion,
    ) {
        if (providerId != provider || goalId != completion.id) return
        goalOutcome = completion.outcome
        if (completion.outcome == PhoneControlUiGoalOutcome.INTERRUPTED && token == null) {
            failNavigation(provider, "goal_interrupted")
            return
        }
        if (token != null) {
            startLocalAdapter(provider)
            return
        }
        postGoalDeadlineMs = SystemClock.elapsedRealtime() + POST_GOAL_SETTLE_MS
        schedule(0L)
    }

    fun cancelIfAuthorityChanged(selectedProvider: String?) {
        if (providerId != selectedProvider) cancelMonitor("authority_changed")
    }

    fun cancel(provider: String?, reason: String) {
        if (provider == null || providerId == provider) cancelMonitor(reason)
    }

    override fun close() {
        cancelMonitor("service_destroyed")
    }

    private fun seal(provider: String) {
        val candidate = runtime() ?: return
        val policy = setup.capturePolicy(provider) ?: return
        val checkpointToken = checkpoint.begin(
            provider,
            policy,
            candidate,
            releaseProjection,
        ) ?: run {
            cancelMonitor("checkpoint_reservation_failed")
            return
        }
        token = checkpointToken
        lastReadiness = "structural_evidence"
        boundaryDeadlineMs = SystemClock.elapsedRealtime() + BOUNDARY_WARNING_MS
        PhoneControlLog.i(
            TAG,
            "protected_checkpoint_detected provider=$provider goal_id=${goalId ?: -1L}",
        )
        val activeGoalId = goalId
        if (goalOutcome == null &&
            (activeGoalId == null || !candidate.requestProtectedCheckpointBoundary(activeGoalId))
        ) {
            PhoneControlLog.w(
                TAG,
                "protected_checkpoint_boundary accepted=false reason=runtime_rejected",
            )
        }
    }

    private fun startLocalAdapter(provider: String) {
        val checkpointToken = token ?: return
        clearMonitorState()
        PhoneControlLog.i(
            TAG,
            "protected_checkpoint_boundary accepted=true provider=$provider " +
                "outcome=${goalOutcome?.name?.lowercase() ?: "none"}",
        )
        setup.start(provider, checkpointToken)
    }

    private fun continueFromFreshEvidence(provider: String): Boolean {
        val previousGoalId = goalId ?: return false
        val nextGoalId = continueNavigation(provider, previousGoalId, lastReadiness)
        if (nextGoalId == null || nextGoalId <= 0L) {
            failNavigation(provider, "continuation_rejected")
            return false
        }
        navigationAttempt += 1
        goalId = nextGoalId
        goalOutcome = null
        postGoalDeadlineMs = 0L
        PhoneControlLog.i(
            TAG,
            "protected_checkpoint_navigation_retry provider=$provider " +
                "goal_id=$nextGoalId attempt=$navigationAttempt reason=$lastReadiness",
        )
        return true
    }

    private fun failNavigation(provider: String, reason: String) {
        PhoneControlLog.w(
            TAG,
            "protected_checkpoint_monitor ended=true provider=$provider reason=$reason",
        )
        clearMonitorState()
        navigationExhausted(provider, reason)
    }

    private fun requestStillValid(provider: String): Boolean =
        runtime() != null &&
            (token != null || projectionActive()) &&
            PhoneControlPowerPreferences.current(context)?.elevatedProviderId == provider

    private fun schedule(delayMs: Long) {
        handler.removeCallbacks(readinessCheck)
        handler.postDelayed(readinessCheck, delayMs)
    }

    private fun cancelMonitor(reason: String) {
        if (providerId == null) return
        PhoneControlLog.i(TAG, "protected_checkpoint_monitor ended=true reason=$reason")
        clearMonitorState()
    }

    private fun clearMonitorState() {
        handler.removeCallbacks(readinessCheck)
        structuralObserver?.close()
        structuralObserver = null
        providerId = null
        goalId = null
        goalOutcome = null
        token = null
        navigationDeadlineMs = 0L
        postGoalDeadlineMs = 0L
        boundaryDeadlineMs = 0L
        navigationAttempt = 0
        lastReadiness = "checkpoint_not_ready"
    }

    private companion object {
        const val TAG = "SGTPhoneControlService"
        const val EVENT_SETTLE_MS = 40L
        const val FALLBACK_CHECK_MS = 500L
        const val POST_GOAL_SETTLE_MS = 1_000L
        const val NAVIGATION_TIMEOUT_MS = 120_000L
        const val BOUNDARY_WARNING_MS = 120_000L
        const val MAX_NAVIGATION_ATTEMPTS = 4
    }
}

internal enum class PhoneControlProtectedNavigationDecision {
    WAIT,
    RETRY,
    EXHAUSTED,
}

internal fun phoneControlProtectedNavigationDecision(
    goalFinished: Boolean,
    postGoalSettled: Boolean,
    attempt: Int,
    maximumAttempts: Int,
    deadlineReached: Boolean,
): PhoneControlProtectedNavigationDecision {
    require(attempt > 0)
    require(maximumAttempts > 0)
    return when {
        deadlineReached -> PhoneControlProtectedNavigationDecision.EXHAUSTED
        !goalFinished || !postGoalSettled -> PhoneControlProtectedNavigationDecision.WAIT
        attempt < maximumAttempts -> PhoneControlProtectedNavigationDecision.RETRY
        else -> PhoneControlProtectedNavigationDecision.EXHAUSTED
    }
}
