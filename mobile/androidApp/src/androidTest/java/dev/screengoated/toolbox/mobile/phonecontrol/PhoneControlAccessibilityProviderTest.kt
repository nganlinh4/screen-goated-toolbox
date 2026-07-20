package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.UiAutomation
import android.content.ComponentName
import android.content.Intent
import android.os.ParcelFileDescriptor
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionVerb
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Intentionally excluded from `run-phone-control-tests.ps1` until a separate driver APK exists.
 * Starting same-target instrumentation force-stops the target-hosted AccessibilityService and
 * prevents a real bind on the API-35 test emulator. The harness uses a host-driven bound-service
 * probe in the meantime.
 */
@RunWith(AndroidJUnit4::class)
class PhoneControlAccessibilityProviderTest {
    @Test
    fun observationTargetsAreInvalidatedWhenFixtureSurfaceChanges() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val targetContext = instrumentation.targetContext
        allowAccessibilityServicesDuringAutomation()
        val component = ComponentName(
            targetContext,
            SgtAccessibilityService::class.java,
        ).flattenToString()
        val originalServices = readSecureSetting(ENABLED_SERVICES)
        val originalEnabled = readSecureSetting(ACCESSIBILITY_ENABLED)

        try {
            enableAccessibilityService(component, originalServices)
            awaitCondition("Accessibility provider did not connect") {
                PhoneControlAccessibilityProvider.isReady
            }
            val intent = Intent(targetContext, PhoneControlAccessibilityFixtureActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            ActivityScenario.launch<PhoneControlAccessibilityFixtureActivity>(intent).use { scenario ->
                instrumentation.waitForIdleSync()
                val first = awaitObservation(
                    PhoneControlAccessibilityFixtureActivity.INITIAL_ACTION_LABEL,
                )
                val oldTarget = first.elements.single { element ->
                    element.label == PhoneControlAccessibilityFixtureActivity.INITIAL_ACTION_LABEL
                }
                assertEquals(targetContext.packageName, oldTarget.packageName)
                assertEquals(first.generation, oldTarget.target.snapshotGeneration)
                val protectedField = first.elements.single { element -> element.isProtected }
                assertTrue(protectedField.isProtected)
                assertNull(protectedField.label)
                assertNull(protectedField.value)
                assertNull(protectedField.hint)
                assertNull(protectedField.stateDescription)
                assertFalse(
                    first.toModelText().contains(
                        PhoneControlAccessibilityFixtureActivity.PROTECTED_FIELD_CANARY,
                    ),
                )
                assertFalse(
                    first.toString().contains(
                        PhoneControlAccessibilityFixtureActivity.PROTECTED_FIELD_CANARY,
                    ),
                )

                scenario.onActivity { activity -> activity.mutateSurface() }
                awaitCondition("Accessibility event did not invalidate the observation") {
                    PhoneControlAccessibilityProvider.observationGeneration > first.generation
                }

                val stale = PhoneControlAccessibilityProvider.act(
                    oldTarget.id,
                    AccessibilityActionVerb.CLICK,
                )
                assertTrue(stale is AccessibilityProviderResult.Failure)
                stale as AccessibilityProviderResult.Failure
                assertEquals("stale_target", stale.code)
                assertTrue(stale.freshObservationRequired)

                val fresh = awaitObservation(
                    PhoneControlAccessibilityFixtureActivity.MUTATED_ACTION_LABEL,
                )
                val freshTarget = fresh.elements.single { element ->
                    element.label == PhoneControlAccessibilityFixtureActivity.MUTATED_ACTION_LABEL
                }
                assertTrue(fresh.generation > first.generation)
                assertEquals(fresh.generation, freshTarget.target.snapshotGeneration)
            }
        } finally {
            restoreSecureSetting(ENABLED_SERVICES, originalServices)
            restoreSecureSetting(ACCESSIBILITY_ENABLED, originalEnabled)
        }
    }

    private suspend fun awaitObservation(label: String): AccessibilityObservation {
        repeat(OBSERVATION_ATTEMPTS) {
            when (val observed = PhoneControlAccessibilityProvider.observe()) {
                is AccessibilityProviderResult.Success -> {
                    if (observed.value.elements.any { element -> element.label == label }) {
                        return observed.value
                    }
                }
                is AccessibilityProviderResult.Failure -> Unit
            }
            delay(POLL_INTERVAL_MS)
        }
        error("Accessibility observation never exposed fixture label: $label")
    }

    private suspend fun awaitCondition(message: String, condition: () -> Boolean) {
        repeat(CONDITION_ATTEMPTS) {
            if (condition()) return
            delay(POLL_INTERVAL_MS)
        }
        error(message)
    }

    private fun enableAccessibilityService(component: String, existing: String?) {
        val enabled = existing.orEmpty()
            .split(':')
            .filter(String::isNotBlank)
            .plus(component)
            .distinct()
            .joinToString(":")
        writeSecureSetting(ENABLED_SERVICES, enabled)
        writeSecureSetting(ACCESSIBILITY_ENABLED, "1")
    }

    private fun readSecureSetting(key: String): String? {
        return shell("settings get secure $key")
            .trimEnd('\r', '\n')
            .takeUnless { value -> value == "null" }
    }

    private fun restoreSecureSetting(key: String, value: String?) {
        if (value == null) {
            shell("settings delete secure $key")
        } else {
            writeSecureSetting(key, value)
        }
    }

    private fun writeSecureSetting(key: String, value: String) {
        shell("settings put secure $key ${shellQuote(value)}")
    }

    private fun shellQuote(value: String): String {
        val escaped = value.replace("'", "'\"'\"'")
        return "'$escaped'"
    }

    private fun shell(command: String): String {
        val descriptor = InstrumentationRegistry.getInstrumentation()
            .getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
            .executeShellCommand(command)
        return ParcelFileDescriptor.AutoCloseInputStream(descriptor)
            .bufferedReader()
            .use { reader -> reader.readText() }
    }

    private fun allowAccessibilityServicesDuringAutomation() {
        InstrumentationRegistry.getInstrumentation()
            .getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
    }

    private companion object {
        private const val ENABLED_SERVICES = "enabled_accessibility_services"
        private const val ACCESSIBILITY_ENABLED = "accessibility_enabled"
        private const val POLL_INTERVAL_MS = 200L
        private const val CONDITION_ATTEMPTS = 50
        private const val OBSERVATION_ATTEMPTS = 50
    }
}
