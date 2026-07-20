package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.accessibilityservice.AccessibilityService
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidSystemNavigationToolHandlerTest {
    @Test
    fun `system key parser accepts only reviewed single-key tokens`() {
        assertEquals(AndroidSystemNavigationKey.HOME, parseAndroidSystemNavigationKey(" HOME "))
        assertEquals(
            AndroidSystemNavigationKey.QUICK_SETTINGS,
            parseAndroidSystemNavigationKey("quick_settings"),
        )
        assertNull(parseAndroidSystemNavigationKey("Ctrl+Home"))
        assertNull(parseAndroidSystemNavigationKey("go home"))
    }

    @Test
    fun `home acts on exact foreground surface without an editor and attaches fresh state`() = runTest {
        val backend = FakeBackend(
            listOf(
                observation(40, window(PACKAGE, active = true)),
                observation(42, window(LAUNCHER, active = true)),
            ),
        )
        val result = AndroidSystemNavigationToolHandler(backend).execute(
            JOB,
            args(AndroidSurfaceIdentity(40, 0, 8, PACKAGE).stableTarget(), "home"),
            AndroidSystemNavigationKey.HOME,
        )

        assertEquals(listOf(AccessibilityService.GLOBAL_ACTION_HOME), backend.actions)
        assertEquals("ok", result.response.string("code"))
        assertEquals("ui.key_action", result.response.string("capability"))
        assertEquals("accessibility", result.response.string("provider"))
        assertEquals("verified", result.response.string("effect_status"))
        assertEquals("42", result.response.string("observation_generation"))
        assertTrue(result.response.boolean("state_reconciled"))
        assertTrue(result.response.boolean("fresh_observation_attached"))
        assertTrue(result.response.string("elements").contains("observation_generation=42"))
        assertTrue(result.mutating)
    }

    @Test
    fun `retired content generation continues only for the same foreground surface`() = runTest {
        val continuedBackend = FakeBackend(
            listOf(
                observation(40, window(PACKAGE, active = true)),
                observation(42, window(LAUNCHER, active = true)),
            ),
        )
        val continued = AndroidSystemNavigationToolHandler(continuedBackend).execute(
            JOB,
            args(AndroidSurfaceIdentity(39, 0, 8, PACKAGE).stableTarget(), "back"),
            AndroidSystemNavigationKey.BACK,
        )
        val staleBackend = FakeBackend(listOf(observation(40, window(PACKAGE, active = true))))
        val stale = AndroidSystemNavigationToolHandler(staleBackend).execute(
            JOB,
            args(AndroidSurfaceIdentity(39, 0, 9, PACKAGE).stableTarget(), "back"),
            AndroidSystemNavigationKey.BACK,
        )
        val backgroundBackend = FakeBackend(
            listOf(
                observation(
                    40,
                    window(PACKAGE, active = false),
                    window(LAUNCHER, id = 9, active = true),
                ),
            ),
        )
        val background = AndroidSystemNavigationToolHandler(backgroundBackend).execute(
            JOB,
            args(AndroidSurfaceIdentity(40, 0, 8, PACKAGE).stableTarget(), "back"),
            AndroidSystemNavigationKey.BACK,
        )

        assertEquals("ok", continued.response.string("code"))
        assertEquals(listOf(AccessibilityService.GLOBAL_ACTION_BACK), continuedBackend.actions)
        assertEquals("stale_target", stale.response.string("code"))
        assertEquals("target_not_foreground", background.response.string("code"))
        assertEquals("proven_no_effect", stale.response.string("effect_status"))
        assertFalse(stale.mutating)
        assertTrue(staleBackend.actions.isEmpty())
        assertTrue(backgroundBackend.actions.isEmpty())
    }

    @Test
    fun `back can leave an exact foreground Android system surface`() = runTest {
        val systemUi = "dev.fixture.systemui"
        val backend = FakeBackend(
            listOf(
                observation(40, window(systemUi, active = true, type = "system")),
                observation(42, window(LAUNCHER, active = true)),
            ),
        )

        val result = AndroidSystemNavigationToolHandler(backend).execute(
            JOB,
            args(AndroidSurfaceIdentity(40, 0, 8, systemUi).stableTarget(), "back"),
            AndroidSystemNavigationKey.BACK,
        )

        assertEquals(listOf(AccessibilityService.GLOBAL_ACTION_BACK), backend.actions)
        assertEquals("ok", result.response.string("code"))
        assertEquals("verified", result.response.string("effect_status"))
    }

    @Test
    fun `system navigation rejects hold duration before dispatch`() = runTest {
        val backend = FakeBackend(listOf(observation(40, window(PACKAGE, active = true))))
        val result = AndroidSystemNavigationToolHandler(backend).execute(
            JOB,
            buildJsonObject {
                put("target", AndroidSurfaceIdentity(40, 0, 8, PACKAGE).stableTarget())
                put("keys", "recents")
                put("hold_seconds", 0.1)
            },
            AndroidSystemNavigationKey.RECENTS,
        )

        assertEquals("invalid_arguments", result.response.string("code"))
        assertTrue(backend.actions.isEmpty())
    }

    private class FakeBackend(
        private val observations: List<AccessibilityObservation>,
    ) : SurfaceToolBackend {
        private var index = 0
        private var generation = observations.first().generation
        val actions = mutableListOf<Int>()

        override val isReady = true
        override val observationGeneration: Long get() = generation

        override suspend fun observe(): AccessibilityProviderResult<AccessibilityObservation> {
            val observation = observations[index.coerceAtMost(observations.lastIndex)]
            if (index < observations.lastIndex) index += 1
            generation = observation.generation
            return AccessibilityProviderResult.Success(observation)
        }

        override fun launchPackage(packageName: String): AndroidProviderResult =
            AndroidProviderResult.Failure("unused", "unused")

        override fun isPackageLaunchable(packageName: String) = false

        override fun appLabel(packageName: String): String? = null

        override fun displayBounds(displayId: Int): TargetBounds? = BOUNDS

        override fun invalidate(reason: String) = Unit

        override suspend fun globalAction(
            lease: AccessibilitySurfaceLease,
            action: Int,
        ): AccessibilityProviderResult<AccessibilityGestureOutcome> {
            actions += action
            generation += 1
            return AccessibilityProviderResult.Success(
                AccessibilityGestureOutcome(
                    code = "ok",
                    generation = generation,
                    effect = EffectCertainty.MAY_HAVE_OCCURRED,
                    snapshotInvalidated = true,
                ),
            )
        }

        override suspend fun postconditionPause() = Unit
    }

    private companion object {
        const val PACKAGE = "dev.fixture.current"
        const val LAUNCHER = "dev.fixture.launcher"
        val BOUNDS = TargetBounds(0, 0, 1080, 2400)
        val JOB = PhoneControlToolJobContext(1, "navigation-job", 1)

        fun args(target: String, keys: String): JsonObject = buildJsonObject {
            put("target", target)
            put("keys", keys)
        }

        fun observation(
            generation: Long,
            vararg windows: AccessibilityWindowSnapshot,
        ) = AccessibilityObservation(
            generation = generation,
            observedAtMs = generation,
            displayRotation = 0,
            densityDpi = 420,
            windows = windows.toList(),
            elements = emptyList(),
            truncated = false,
        )

        fun window(
            packageName: String,
            id: Int = 8,
            active: Boolean,
            type: String = "application",
        ) = AccessibilityWindowSnapshot(
            id = id,
            displayId = 0,
            layer = 1,
            type = type,
            title = packageName,
            packageName = packageName,
            active = active,
            focused = active,
            bounds = BOUNDS,
        )
    }
}

private fun JsonObject.string(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.boolean(name: String): Boolean = string(name).toBoolean()
