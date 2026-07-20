package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.view.KeyEvent
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityKeyGroup
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTextOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTextTarget
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTargetAuthority
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TextToolHandlersTest {
    @Test
    fun `type text binds current observed id and preserves explicit options`() = runTest {
        val backend = FakeTextBackend()
        val handlers = TextToolHandlers(missingArtifactResolver, backend)

        val result = handlers.typeText(
            job,
            buildJsonObject {
                put("target", surfaceTarget)
                put("text", "Xin chào 👋")
                put("slow", true)
                put("press_enter", false)
            },
        )

        assertEquals("ok", result.response.stringValue("code"))
        assertEquals("verified", result.response.stringValue("effect_status"))
        assertEquals(textTarget, backend.typedTarget)
        assertEquals(surfaceIdentity, backend.requestedSurface)
        assertEquals("Xin chào 👋", backend.typedText)
        assertTrue(backend.typedSlow)
        assertFalse(backend.typedPressEnter)
        assertEquals("10", result.response.stringValue("dispatched_code_points"))
        assertEquals("true", result.response.stringValue("text_postcondition_verified"))
        assertFalse(result.response.containsKey("inserted_code_points"))
    }

    @Test
    fun `unacknowledged input dispatch never claims a verified text postcondition`() = runTest {
        val backend = FakeTextBackend().apply {
            typeResult = AccessibilityProviderResult.Success(
                successOutcome(inserted = 5).copy(effect = EffectCertainty.MAY_HAVE_OCCURRED),
            )
        }

        val result = TextToolHandlers(missingArtifactResolver, backend).typeText(
            job,
            buildJsonObject {
                put("target", surfaceTarget)
                put("text", "hello")
            },
        )

        assertEquals("may_have_occurred", result.response.stringValue("effect_status"))
        assertEquals("5", result.response.stringValue("dispatched_code_points"))
        assertEquals("false", result.response.stringValue("text_postcondition_verified"))
        assertTrue(result.mutating)
    }

    @Test
    fun `desktop window token cannot bypass Android focused target identity`() = runTest {
        val backend = FakeTextBackend()
        val handlers = TextToolHandlers(missingArtifactResolver, backend)

        val result = handlers.typeText(
            job,
            buildJsonObject {
                put("target", "@hwnd:123:456")
                put("text", "hello")
            },
        )

        assertEquals("invalid_arguments", result.response.stringValue("code"))
        assertEquals(null, backend.typedTarget)
        assertEquals("proven_no_effect", result.response.stringValue("effect_status"))
    }

    @Test
    fun `stale focused editor returns no effect and requires observation`() = runTest {
        val backend = FakeTextBackend().apply {
            focusResult = AccessibilityProviderResult.Failure(
                "stale_target",
                "stale",
                retryable = true,
                freshObservationRequired = true,
            )
        }
        val result = TextToolHandlers(missingArtifactResolver, backend).typeText(
            job,
            buildJsonObject {
                put("target", surfaceTarget)
                put("text", "hello")
            },
        )

        assertEquals("stale_target", result.response.stringValue("code"))
        assertEquals("proven_no_effect", result.response.stringValue("effect_status"))
        assertEquals("true", result.response.stringValue("fresh_observation_required"))
        assertFalse(result.mutating)
        assertEquals(null, backend.typedTarget)
    }

    @Test
    fun `paste resolves artifact locally and inserts through current focused editor`() = runTest {
        val backend = FakeTextBackend()
        val resolver = TextArtifactResolver { id ->
            TextArtifactResolution.Success(id, "large exact text", "abc123", 16)
        }
        val result = TextToolHandlers(resolver, backend).pasteArtifact(
            job,
            buildJsonObject { put("id", "artifact-7") },
        )

        assertEquals("large exact text", backend.typedText)
        assertEquals(textTarget, backend.typedTarget)
        assertEquals("artifact-7", result.response.stringValue("artifact_id"))
        assertEquals("abc123", result.response.stringValue("source_sha256"))
        assertFalse(result.response.toString().contains("large exact text"))
    }

    @Test
    fun `artifact failure never probes or mutates the focused editor`() = runTest {
        val backend = FakeTextBackend()
        val resolver = TextArtifactResolver {
            TextArtifactResolution.Failure("not_utf8", "invalid")
        }
        val result = TextToolHandlers(resolver, backend).pasteArtifact(
            job,
            buildJsonObject { put("id", "artifact-7") },
        )

        assertEquals("not_utf8", result.response.stringValue("code"))
        assertEquals("android_app_api", result.response.stringValue("provider"))
        assertEquals("dependency", result.response.stringValue("provider_role"))
        assertEquals(0, backend.focusQueries)
        assertEquals(null, backend.typedTarget)
    }

    @Test
    fun `Android chord parser preserves sequences and rejects desktop-only key`() = runTest {
        val backend = FakeTextBackend()
        val handlers = TextToolHandlers(missingArtifactResolver, backend)
        val supported = handlers.keyCombination(
            job,
            buildJsonObject {
                put("target", surfaceTarget)
                put("keys", "Ctrl+Shift+K, Tab")
                put("hold_seconds", 0.1)
            },
        )
        val unsupported = handlers.keyCombination(
            job,
            buildJsonObject {
                put("target", surfaceTarget)
                put("keys", "Win+R")
            },
        )

        assertEquals("ok", supported.response.stringValue("code"))
        assertEquals(100L, backend.keyHoldMs)
        assertEquals(
            listOf(
                AccessibilityKeyGroup(
                    listOf(KeyEvent.KEYCODE_CTRL_LEFT, KeyEvent.KEYCODE_SHIFT_LEFT, KeyEvent.KEYCODE_K),
                ),
                AccessibilityKeyGroup(listOf(KeyEvent.KEYCODE_TAB)),
            ),
            backend.keyGroups,
        )
        assertEquals("unsupported_on_android", unsupported.response.stringValue("code"))
        assertEquals("unsupported", unsupported.response.stringValue("provider_state"))
    }

    @Test
    fun `registry declares all three text tools as real mutating handlers`() {
        val expected = mapOf(
            "type_text" to PhoneControlHandler.TYPE_TEXT,
            "key_combination" to PhoneControlHandler.KEY_COMBINATION,
            "paste_artifact" to PhoneControlHandler.PASTE_ARTIFACT,
        )
        expected.forEach { (name, handler) ->
            val spec = PhoneControlToolRegistry.byName[name]
            assertNotNull(spec)
            assertEquals(handler, spec?.handler)
            assertTrue(spec?.handler?.mutating == true)
        }
    }

    private class FakeTextBackend : TextToolBackend {
        override val isReady = true
        override val observationGeneration = 44L
        var focusQueries = 0
        var requestedSurface: AndroidSurfaceIdentity? = null
        var typedTarget: AccessibilityTextTarget? = null
        var typedText: String? = null
        var typedSlow = false
        var typedPressEnter = false
        var keyGroups: List<AccessibilityKeyGroup>? = null
        var keyHoldMs: Long? = null
        var focusResult: AccessibilityProviderResult<AccessibilityTextTarget> =
            AccessibilityProviderResult.Success(textTarget)
        var typeResult: AccessibilityProviderResult<AccessibilityTextOutcome> =
            AccessibilityProviderResult.Success(successOutcome(inserted = 10))

        override suspend fun focusedTarget(
            surface: AndroidSurfaceIdentity?,
        ): AccessibilityProviderResult<AccessibilityTextTarget> {
            focusQueries += 1
            requestedSurface = surface
            return focusResult
        }

        override suspend fun typeText(
            target: AccessibilityTextTarget,
            text: String,
            slow: Boolean,
            pressEnter: Boolean,
        ): AccessibilityProviderResult<AccessibilityTextOutcome> {
            typedTarget = target
            typedText = text
            typedSlow = slow
            typedPressEnter = pressEnter
            return typeResult
        }

        override suspend fun sendKeys(
            target: AccessibilityTextTarget,
            groups: List<AccessibilityKeyGroup>,
            holdMs: Long,
        ): AccessibilityProviderResult<AccessibilityTextOutcome> {
            typedTarget = target
            keyGroups = groups
            keyHoldMs = holdMs
            return AccessibilityProviderResult.Success(
                successOutcome(inserted = 0, completedGroups = groups.size),
            )
        }
    }

    private companion object {
        val job = PhoneControlToolJobContext(1L, "job-text", 1L)
        val missingArtifactResolver = TextArtifactResolver {
            TextArtifactResolution.Failure("artifact_not_found", "missing")
        }
        val identity = PhoneControlTargetIdentity(
            snapshotGeneration = 44L,
            displayId = 0,
            windowId = 8L,
            packageOrSurface = "fixture.package",
            nodeOrDocumentIdentity = "8:0",
            bounds = TargetBounds(0, 0, 100, 100),
            observationTimestampMs = 1L,
        )
        val textTarget = AccessibilityTextTarget(
            id = 12,
            identity = identity,
            authority = AccessibilityTargetAuthority.ROUTINE,
        )
        val surfaceIdentity = AndroidSurfaceIdentity(
            generation = 44L,
            displayId = 0,
            windowId = 8L,
            packageName = "fixture.package",
        )
        val surfaceTarget = surfaceIdentity.stableTarget()

        fun successOutcome(inserted: Int, completedGroups: Int = 0) = AccessibilityTextOutcome(
            code = "ok",
            provider = "accessibility_input_method",
            generation = 45L,
            effect = EffectCertainty.VERIFIED,
            snapshotInvalidated = true,
            freshObservationRequired = true,
            insertedCodePoints = inserted,
            completedKeyGroups = completedGroups,
        )
    }
}

private fun JsonObject.stringValue(key: String): String? =
    (get(key) as? JsonPrimitive)?.jsonPrimitive?.content
