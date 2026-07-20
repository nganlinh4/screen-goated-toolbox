package dev.screengoated.toolbox.mobile.service.nativelibs

import android.content.Context
import android.os.Handler
import android.os.Looper
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepSessionRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepToken
import java.util.IdentityHashMap

internal object PlaySplitInstallConfirmationCoordinator {
    private val lock = Any()
    private val mainHandler = Handler(Looper.getMainLooper())
    private val activeTokens = mutableMapOf<Int, PlatformUserStepToken>()
    private val confirmationLatch = PlaySplitConfirmationLatch()
    private val failureHandlers =
        mutableMapOf<Int, IdentityHashMap<Any, (String) -> Unit>>()

    fun request(
        context: Context,
        sessionId: Int,
        owner: Any,
        onFailure: (String) -> Unit,
    ) {
        val shouldLaunch = synchronized(lock) {
            failureHandlers.getOrPut(sessionId) { IdentityHashMap() }[owner] = onFailure
            if (!confirmationLatch.request(sessionId)) {
                false
            } else {
                ensureTokenLocked(sessionId)
                true
            }
        }
        if (!shouldLaunch) return
        mainHandler.post { launchIfPending(context.applicationContext, sessionId) }
    }

    fun ensureActive(sessionId: Int): Boolean = synchronized(lock) {
        if (sessionId <= 0) return@synchronized false
        if (!confirmationLatch.restorePending(sessionId)) return@synchronized false
        ensureTokenLocked(sessionId)
        true
    }

    fun promptResolved(sessionId: Int, accepted: Boolean) {
        if (accepted) {
            synchronized(lock) {
                confirmationLatch.markAccepted(sessionId)
            }
            endToken(sessionId)
        } else {
            fail(sessionId, "Play feature install confirmation was declined")
        }
    }

    fun promptNoLongerRequired(sessionId: Int) {
        resetRequest(sessionId)
    }

    fun resetRequest(sessionId: Int) {
        synchronized(lock) {
            confirmationLatch.clearRequirement(sessionId)
        }
        endToken(sessionId)
    }

    fun fail(sessionId: Int, message: String) {
        val handlers = synchronized(lock) {
            confirmationLatch.clearRequirement(sessionId)
            val token = activeTokens.remove(sessionId)
            if (token != null) PlatformUserStepSessionRegistry.end(token)
            failureHandlers.remove(sessionId)?.values.orEmpty().toList()
        }
        handlers.forEach { handler -> runCatching { handler(message) } }
    }

    fun release(sessionId: Int) {
        synchronized(lock) {
            confirmationLatch.clearRequirement(sessionId)
            failureHandlers.remove(sessionId)
            val token = activeTokens.remove(sessionId)
            if (token != null) PlatformUserStepSessionRegistry.end(token)
        }
    }

    private fun launchIfPending(context: Context, sessionId: Int) {
        val pending = synchronized(lock) { confirmationLatch.isPending(sessionId) }
        if (!pending) return
        runCatching {
            context.startActivity(PlaySplitInstallConfirmationActivity.intent(context, sessionId))
        }.onFailure { error ->
            fail(sessionId, error.message ?: "Unable to open Play feature confirmation")
        }
    }

    private fun ensureTokenLocked(sessionId: Int) {
        activeTokens.getOrPut(sessionId, PlatformUserStepSessionRegistry::begin)
    }

    private fun endToken(sessionId: Int) {
        val token = synchronized(lock) { activeTokens.remove(sessionId) }
        if (token != null) PlatformUserStepSessionRegistry.end(token)
    }
}
