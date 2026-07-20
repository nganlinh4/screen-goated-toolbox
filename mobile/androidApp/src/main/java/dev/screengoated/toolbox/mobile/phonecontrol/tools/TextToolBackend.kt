package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.os.Build
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityKeyGroup
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTextOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTextTarget
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider

internal interface TextToolBackend {
    val isReady: Boolean
    val observationGeneration: Long
    val textInputProviderId: String
        get() = TextProviderIds.ACCESSIBILITY

    suspend fun focusedTarget(
        surface: AndroidSurfaceIdentity? = null,
    ): AccessibilityProviderResult<AccessibilityTextTarget>

    suspend fun typeText(
        target: AccessibilityTextTarget,
        text: String,
        slow: Boolean,
        pressEnter: Boolean,
    ): AccessibilityProviderResult<AccessibilityTextOutcome>

    suspend fun sendKeys(
        target: AccessibilityTextTarget,
        groups: List<AccessibilityKeyGroup>,
        holdMs: Long,
    ): AccessibilityProviderResult<AccessibilityTextOutcome>
}

internal object AndroidTextToolBackend : TextToolBackend {
    override val isReady: Boolean
        get() = PhoneControlAccessibilityProvider.isReady
    override val observationGeneration: Long
        get() = PhoneControlAccessibilityProvider.observationGeneration
    override val textInputProviderId: String
        get() = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            TextProviderIds.INPUT_METHOD
        } else {
            TextProviderIds.ACCESSIBILITY
        }

    override suspend fun focusedTarget(
        surface: AndroidSurfaceIdentity?,
    ): AccessibilityProviderResult<AccessibilityTextTarget> =
        PhoneControlAccessibilityProvider.focusedTextTarget(surface)

    override suspend fun typeText(
        target: AccessibilityTextTarget,
        text: String,
        slow: Boolean,
        pressEnter: Boolean,
    ): AccessibilityProviderResult<AccessibilityTextOutcome> =
        PhoneControlAccessibilityProvider.typeText(target, text, slow, pressEnter)

    override suspend fun sendKeys(
        target: AccessibilityTextTarget,
        groups: List<AccessibilityKeyGroup>,
        holdMs: Long,
    ): AccessibilityProviderResult<AccessibilityTextOutcome> =
        PhoneControlAccessibilityProvider.sendKeys(target, groups, holdMs)
}

internal object TextProviderIds {
    const val ACCESSIBILITY = "accessibility"
    const val INPUT_METHOD = "accessibility_input_method"
}
