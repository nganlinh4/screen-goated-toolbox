package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.effect.currentPhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.util.concurrent.atomic.AtomicBoolean

internal class OwnedAccessibilityEffect private constructor(
    private val ownerPresent: Boolean,
    private val lease: PhoneControlEffectOwner.EffectLease?,
) : AutoCloseable {
    private val platformAccepted = AtomicBoolean(false)

    val wasAccepted: Boolean
        get() = platformAccepted.get()

    fun dispatchBoolean(dispatch: () -> Boolean): Boolean? {
        val accepted = if (ownerPresent) lease?.dispatchBooleanIfActive(dispatch) else dispatch()
        if (accepted == true) platformAccepted.set(true)
        return accepted
    }

    fun dispatch(dispatch: () -> Unit): Boolean {
        val dispatched = if (ownerPresent) lease?.dispatchIfActive(dispatch) == true else {
            dispatch()
            true
        }
        if (dispatched) platformAccepted.set(true)
        return dispatched
    }

    override fun close() {
        lease?.close()
    }

    companion object {
        suspend fun begin(): OwnedAccessibilityEffect {
            val owner = currentPhoneControlEffectOwner()
                ?: return OwnedAccessibilityEffect(ownerPresent = false, lease = null)
            return OwnedAccessibilityEffect(ownerPresent = true, lease = owner.beginEffect())
        }
    }
}

internal fun <T> cancelledBeforeDispatch(): AccessibilityProviderResult<T> =
    AccessibilityProviderResult.Failure(
        code = "operation_cancelled",
        message = "The owned operation was cancelled before Android accepted an effect.",
        retryable = false,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
    )
