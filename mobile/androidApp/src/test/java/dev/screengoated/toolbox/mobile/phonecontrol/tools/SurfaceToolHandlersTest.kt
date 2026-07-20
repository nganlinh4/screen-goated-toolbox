package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.accessibilityservice.AccessibilityService
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlAuthorityFixture
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouter
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
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SurfaceToolHandlersTest {
    @Test
    fun listPublishesCurrentScopeStableTargetsAndBlindWindows() = runTest {
        val backend = FakeSurfaceBackend(listOf(observation(7, app(active = true), blindWindow())))
        val execution = SurfaceToolHandlers(backend).listWindows(JOB)
        val windows = execution.response.getValue("windows").jsonArray

        assertEquals("ok", execution.response.string("code"))
        assertEquals("current_interactive_surfaces_only", execution.response.string("visibility_scope"))
        assertEquals(2, windows.size)
        assertEquals(
            AndroidSurfaceIdentity(7, 0, 10, PACKAGE).stableTarget(),
            windows[0].jsonObject.string("target"),
        )
        assertTrue(windows[0].jsonObject.boolean("targetable"))
        assertFalse(windows[1].jsonObject.boolean("content_accessible"))
        assertFalse(windows[1].jsonObject.boolean("targetable"))
    }

    @Test
    fun activeWindowQueryFiltersWithoutInventingTaskHistory() = runTest {
        val backend = FakeSurfaceBackend(
            listOf(observation(4, app(active = true), app(id = 11, packageName = "other", active = false))),
        )
        val execution = SurfaceToolHandlers(backend).queryWindows(JOB, "active")

        assertEquals(1, execution.response.getValue("windows").jsonArray.size)
        assertEquals("system_query", execution.response.string("requested_tool"))
    }

    @Test
    fun staleStableTargetFailsBeforeLaunching() = runTest {
        val backend = FakeSurfaceBackend(listOf(observation(9, app(active = false))))
        val stale = AndroidSurfaceIdentity(8, 0, 10, PACKAGE).stableTarget()
        val result = SurfaceToolHandlers(backend).focusWindow(JOB, titleArgs(stale))

        assertEquals("stale_target", result.response.string("code"))
        assertEquals("proven_no_effect", result.response.string("effect_status"))
        assertEquals(0, backend.launches)
    }

    @Test
    fun duplicateExactTitlesReturnStableChoicesWithoutActing() = runTest {
        val backend = FakeSurfaceBackend(
            listOf(
                observation(
                    5,
                    app(id = 10, packageName = "one", title = "Shared", active = false),
                    app(id = 12, packageName = "two", title = "Ｓｈａｒｅｄ", active = false),
                ),
            ),
        )
        val result = SurfaceToolHandlers(backend).focusWindow(JOB, titleArgs(" shared "))

        assertEquals("ambiguous_target", result.response.string("code"))
        assertEquals(2, result.response.getValue("choices").jsonArray.size)
        assertEquals(0, backend.launches)
    }

    @Test
    fun focusReturnsNoEffectWhenAlreadyFocusedAndVerifiesLaunchOtherwise() = runTest {
        val activeBackend = FakeSurfaceBackend(listOf(observation(3, app(active = true))))
        val target = AndroidSurfaceIdentity(3, 0, 10, PACKAGE).stableTarget()
        val already = SurfaceToolHandlers(activeBackend).focusWindow(JOB, titleArgs(target))
        assertEquals("proven_no_effect", already.response.string("effect_status"))
        assertEquals("accessibility", already.response.string("provider"))
        assertEquals(0, activeBackend.launches)

        val launchBackend = FakeSurfaceBackend(
            listOf(
                observation(3, app(active = false)),
                observation(4, app(active = true)),
            ),
        )
        val focused = SurfaceToolHandlers(launchBackend).focusWindow(JOB, titleArgs(target))
        assertEquals("verified", focused.response.string("effect_status"))
        assertEquals("android_app_api", focused.response.string("provider"))
        assertEquals(1, launchBackend.launches)
        assertEquals(PACKAGE, focused.response.getValue("surface").jsonObject.string("package"))
    }

    @Test
    fun unverifiedFocusReportsPossibleEffectAndInvalidatesSnapshot() = runTest {
        val backend = FakeSurfaceBackend(listOf(observation(2, app(active = false))))
        val target = AndroidSurfaceIdentity(2, 0, 10, PACKAGE).stableTarget()
        val result = SurfaceToolHandlers(backend).focusWindow(JOB, titleArgs(target))

        assertEquals("focus_not_verified", result.response.string("code"))
        assertEquals("android_app_api", result.response.string("provider"))
        assertEquals("may_have_occurred", result.response.string("effect_status"))
        assertTrue(result.response.boolean("snapshot_invalidated"))
    }

    @Test
    fun focusRegistryDeclaresBothActualPrimaryProvidersInRouteOrder() {
        val spec = PhoneControlToolRegistry.byName.getValue("focus_window")

        assertEquals(listOf("android_app_api", "accessibility"), spec.providerIds)
        assertTrue(spec.dependencyProviderIds.isEmpty())
    }

    @Test
    fun focusReceiptsRemainAttestedAcrossBothPrimaryAuthorities() = runTest {
        val observationFailure = dispatchFocus(
            FakeSurfaceBackend(
                observations = listOf(observation(1, app(active = false))),
                observationFailure = AccessibilityProviderResult.Failure(
                    code = "observation_failed",
                    message = "fixture failure",
                    retryable = true,
                ),
            ),
            "Fixture",
        )
        val alreadyFocused = dispatchFocus(
            FakeSurfaceBackend(listOf(observation(3, app(active = true)))),
            AndroidSurfaceIdentity(3, 0, 10, PACKAGE).stableTarget(),
        )
        val notLaunchable = dispatchFocus(
            FakeSurfaceBackend(
                observations = listOf(observation(2, app(active = false))),
                launchable = false,
            ),
            AndroidSurfaceIdentity(2, 0, 10, PACKAGE).stableTarget(),
        )
        val launchFailure = dispatchFocus(
            FakeSurfaceBackend(
                observations = listOf(observation(4, app(active = false))),
                launchResult = AndroidProviderResult.Failure(
                    code = "launch_failed",
                    message = "fixture failure",
                ),
            ),
            AndroidSurfaceIdentity(4, 0, 10, PACKAGE).stableTarget(),
        )
        val verified = dispatchFocus(
            FakeSurfaceBackend(
                listOf(
                    observation(5, app(active = false)),
                    observation(6, app(active = true)),
                ),
            ),
            AndroidSurfaceIdentity(5, 0, 10, PACKAGE).stableTarget(),
        )
        val unverified = dispatchFocus(
            FakeSurfaceBackend(listOf(observation(7, app(active = false)))),
            AndroidSurfaceIdentity(7, 0, 10, PACKAGE).stableTarget(),
        )

        assertAttested(observationFailure, "accessibility", "proven_no_effect")
        assertAttested(alreadyFocused, "accessibility", "proven_no_effect")
        assertAttested(notLaunchable, "android_app_api", "proven_no_effect")
        assertAttested(launchFailure, "android_app_api", "proven_no_effect")
        assertAttested(verified, "android_app_api", "verified")
        assertAttested(unverified, "android_app_api", "may_have_occurred")
    }

    @Test
    fun visibleApplicationWithoutLauncherIsRejectedBeforeDispatch() = runTest {
        val backend = FakeSurfaceBackend(
            observations = listOf(observation(2, app(active = false))),
            launchable = false,
        )
        val target = AndroidSurfaceIdentity(2, 0, 10, PACKAGE).stableTarget()

        val result = SurfaceToolHandlers(backend).focusWindow(JOB, titleArgs(target))

        assertEquals("unsupported_on_surface", result.response.string("code"))
        assertEquals("android_app_api", result.response.string("provider"))
        assertEquals("proven_no_effect", result.response.string("effect_status"))
        assertEquals(0, backend.launches)
    }

    @Test
    fun focusNeverVerifiesAnotherSamePackageSurface() = runTest {
        val target = AndroidSurfaceIdentity(3, 0, 10, PACKAGE).stableTarget()
        val backend = FakeSurfaceBackend(
            listOf(
                observation(3, app(id = 10, title = "Requested", active = false)),
                observation(4, app(id = 12, title = "Other", active = true)),
            ),
        )

        val result = SurfaceToolHandlers(backend).focusWindow(JOB, titleArgs(target))

        assertEquals("focus_not_verified", result.response.string("code"))
        assertEquals("may_have_occurred", result.response.string("effect_status"))
    }

    @Test
    fun focusDoesNotTrustAReusedWindowIdWithADifferentTitle() = runTest {
        val target = AndroidSurfaceIdentity(3, 0, 10, PACKAGE).stableTarget()
        val backend = FakeSurfaceBackend(
            listOf(
                observation(3, app(id = 10, title = "Requested", active = false)),
                observation(4, app(id = 10, title = "Different", active = true)),
            ),
        )

        val result = SurfaceToolHandlers(backend).focusWindow(JOB, titleArgs(target))

        assertEquals("focus_not_verified", result.response.string("code"))
        assertEquals("may_have_occurred", result.response.string("effect_status"))
    }

    @Test
    fun focusAcceptsRetainedIdOrUniqueNormalizedTitleOnlyOnRequestedDisplay() = runTest {
        val target = AndroidSurfaceIdentity(3, 0, 10, PACKAGE).stableTarget()
        val byTitle = FakeSurfaceBackend(
            listOf(
                observation(3, app(id = 10, title = "Requested Surface", active = false)),
                observation(4, app(id = 12, title = "  ＲＥＱＵＥＳＴＥＤ  Surface ", active = true)),
            ),
        )
        val wrongDisplay = FakeSurfaceBackend(
            listOf(
                observation(3, app(id = 10, title = "Requested Surface", active = false)),
                observation(4, app(id = 12, displayId = 1, title = "Requested Surface", active = true)),
            ),
        )

        val verified = SurfaceToolHandlers(byTitle).focusWindow(JOB, titleArgs(target))
        val rejected = SurfaceToolHandlers(wrongDisplay).focusWindow(JOB, titleArgs(target))

        assertEquals("verified", verified.response.string("effect_status"))
        assertEquals("0", verified.response.getValue("surface").jsonObject.string("display_id"))
        assertEquals("focus_not_verified", rejected.response.string("code"))
    }

    @Test
    fun ambiguousFreshFocusReturnsChoicesAndPossibleEffect() = runTest {
        val target = AndroidSurfaceIdentity(3, 0, 10, PACKAGE).stableTarget()
        val backend = FakeSurfaceBackend(
            listOf(
                observation(3, app(id = 10, title = "Requested", active = false)),
                observation(
                    4,
                    app(id = 12, title = "Requested", active = true),
                    app(id = 13, title = "Ｒｅｑｕｅｓｔｅｄ", active = true),
                ),
            ),
        )

        val result = SurfaceToolHandlers(backend).focusWindow(JOB, titleArgs(target))

        assertEquals("focus_ambiguous_postcondition", result.response.string("code"))
        assertEquals("may_have_occurred", result.response.string("effect_status"))
        assertEquals(2, result.response.getValue("choices").jsonArray.size)
    }

    @Test
    fun minimizeRequiresOneForegroundTaskCoverageAndVerifiesHome() = runTest {
        val target = AndroidSurfaceIdentity(6, 0, 10, PACKAGE).stableTarget()
        val successBackend = FakeSurfaceBackend(
            listOf(
                observation(6, app(active = true)),
                observation(7, app(id = 20, packageName = "launcher", active = true)),
            ),
        )
        val success = SurfaceToolHandlers(successBackend).minimizeWindow(JOB, titleArgs(target))
        assertEquals("verified", success.response.string("effect_status"))
        assertEquals(listOf(AccessibilityService.GLOBAL_ACTION_HOME), successBackend.globalActions)

        val splitBackend = FakeSurfaceBackend(
            listOf(
                observation(
                    6,
                    app(active = true),
                    app(id = 11, packageName = "other", active = false),
                    splitDivider(),
                ),
            ),
        )
        val rejected = SurfaceToolHandlers(splitBackend).minimizeWindow(JOB, titleArgs(target))
        assertEquals("unsupported_on_surface", rejected.response.string("code"))
        assertTrue(splitBackend.globalActions.isEmpty())
    }

    @Test
    fun minimizeAcceptsSamePackageEmbeddedPanesButRejectsGapsAndMixedApps() = runTest {
        val target = AndroidSurfaceIdentity(6, 0, 10, PACKAGE).stableTarget()
        val panes = FakeSurfaceBackend(
            listOf(
                observation(
                    6,
                    app(active = true, bounds = TargetBounds(0, 0, 400, 1800)),
                    app(id = 11, active = false, bounds = TargetBounds(400, 0, 1000, 1800)),
                ),
                observation(7, app(id = 20, packageName = "launcher", active = true)),
            ),
        )
        val gap = FakeSurfaceBackend(
            listOf(
                observation(
                    6,
                    app(active = true, bounds = TargetBounds(0, 0, 400, 1800)),
                    app(id = 11, active = false, bounds = TargetBounds(401, 0, 1000, 1800)),
                ),
            ),
        )
        val mixed = FakeSurfaceBackend(
            listOf(
                observation(
                    6,
                    app(active = true, bounds = TargetBounds(0, 0, 400, 1800)),
                    app(
                        id = 11,
                        packageName = "other",
                        active = false,
                        bounds = TargetBounds(400, 0, 1000, 1800),
                    ),
                ),
            ),
        )

        val accepted = SurfaceToolHandlers(panes).minimizeWindow(JOB, titleArgs(target))
        val gapRejected = SurfaceToolHandlers(gap).minimizeWindow(JOB, titleArgs(target))
        val mixedRejected = SurfaceToolHandlers(mixed).minimizeWindow(JOB, titleArgs(target))

        assertEquals("verified", accepted.response.string("effect_status"))
        listOf(gapRejected, mixedRejected).forEach { result ->
            assertEquals("unsupported_on_surface", result.response.string("code"))
            assertEquals("proven_no_effect", result.response.string("effect_status"))
        }
        assertTrue(gap.globalActions.isEmpty())
        assertTrue(mixed.globalActions.isEmpty())
    }

    @Test
    fun minimizeRejectsFreeformSecondaryAndPictureInPictureBeforeHome() = runTest {
        val freeform = FakeSurfaceBackend(
            listOf(
                observation(
                    6,
                    app(active = true, bounds = TargetBounds(100, 100, 900, 1500)),
                ),
            ),
        )
        val secondary = FakeSurfaceBackend(
            listOf(observation(6, app(displayId = 1, active = true))),
            displayBounds = mapOf(0 to DISPLAY_BOUNDS, 1 to DISPLAY_BOUNDS),
        )
        val pictureInPicture = FakeSurfaceBackend(
            listOf(observation(6, app(active = true, pictureInPicture = true))),
        )

        val freeformResult = SurfaceToolHandlers(freeform).minimizeWindow(
            JOB,
            titleArgs(AndroidSurfaceIdentity(6, 0, 10, PACKAGE).stableTarget()),
        )
        val secondaryResult = SurfaceToolHandlers(secondary).minimizeWindow(
            JOB,
            titleArgs(AndroidSurfaceIdentity(6, 1, 10, PACKAGE).stableTarget()),
        )
        val pipResult = SurfaceToolHandlers(pictureInPicture).minimizeWindow(
            JOB,
            titleArgs(AndroidSurfaceIdentity(6, 0, 10, PACKAGE).stableTarget()),
        )

        listOf(freeformResult, secondaryResult, pipResult).forEach { result ->
            assertEquals("unsupported_on_surface", result.response.string("code"))
            assertEquals("proven_no_effect", result.response.string("effect_status"))
        }
        assertTrue(freeform.globalActions.isEmpty())
        assertTrue(secondary.globalActions.isEmpty())
        assertTrue(pictureInPicture.globalActions.isEmpty())
    }

    @Test
    fun geometryToolsRemainExplicitTypedNoEffect() {
        val backend = FakeSurfaceBackend(listOf(observation(1, app(active = true))))
        val result = SurfaceToolHandlers(backend).unsupportedGeometry(
            JOB,
            "resize_window",
            buildJsonObject {
                put("title", PACKAGE)
                put("width", 800)
                put("height", 600)
            },
        )

        assertEquals("unsupported_on_surface", result.response.string("code"))
        assertEquals("unsupported", result.response.string("provider_state"))
        assertFalse(result.mutating)
    }

    private class FakeSurfaceBackend(
        private val observations: List<AccessibilityObservation>,
        private val launchable: Boolean = true,
        private val displayBounds: Map<Int, TargetBounds> = mapOf(0 to DISPLAY_BOUNDS),
        private val observationFailure: AccessibilityProviderResult.Failure? = null,
        private val launchResult: AndroidProviderResult? = null,
    ) : SurfaceToolBackend {
        private var index = 0
        private var generation = observations.first().generation
        var launches = 0
        val globalActions = mutableListOf<Int>()

        override val isReady = true
        override val observationGeneration: Long
            get() = generation

        override suspend fun observe(): AccessibilityProviderResult<AccessibilityObservation> {
            observationFailure?.let { return it }
            val current = observations[index.coerceAtMost(observations.lastIndex)]
            if (index < observations.lastIndex) index += 1
            generation = current.generation
            return AccessibilityProviderResult.Success(current)
        }

        override fun launchPackage(packageName: String): AndroidProviderResult {
            launches += 1
            return launchResult
                ?: AndroidProviderResult.Success(buildJsonObject { put("package", packageName) }, true)
        }

        override fun isPackageLaunchable(packageName: String): Boolean = launchable

        override fun appLabel(packageName: String): String? = packageName.takeIf(String::isNotBlank)

        override fun displayBounds(displayId: Int): TargetBounds? = displayBounds[displayId]

        override fun invalidate(reason: String) {
            generation += 1
        }

        override suspend fun globalAction(
            lease: AccessibilitySurfaceLease,
            action: Int,
        ): AccessibilityProviderResult<AccessibilityGestureOutcome> {
            globalActions += action
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
        const val PACKAGE = "dev.fixture.app"
        val JOB = PhoneControlToolJobContext(1, "surface-job", 1)
        val ROUTER = PhoneControlAuthorityFixture.load().let {
            ProviderRouter(it.providers, it.routes)
        }

        suspend fun dispatchFocus(
            backend: SurfaceToolBackend,
            title: String,
        ): PhoneControlToolExecution {
            val handler = SurfaceToolHandlers(backend)
            val dispatcher = PhoneControlToolDispatcher(
                executor = PhoneControlHandlerExecutor { selected, job, requestedTool, args ->
                    assertEquals(PhoneControlHandler.FOCUS_WINDOW, selected)
                    assertEquals("focus_window", requestedTool)
                    handler.focusWindow(job, args)
                },
                providerRouter = ROUTER,
                failureReporter = PhoneControlToolFailureReporter { _, _, error -> throw error },
            )
            return dispatcher.dispatch(JOB, "focus_window", titleArgs(title))
        }

        fun assertAttested(
            execution: PhoneControlToolExecution,
            provider: String,
            effect: String,
        ) {
            assertEquals(provider, execution.response.string("provider"))
            assertEquals(effect, execution.response.string("effect_status"))
            assertFalse(execution.response.string("code") == "provider_contract_failure")
            assertFalse(execution.response.containsKey("provider_role"))
        }

        fun titleArgs(title: String): JsonObject = buildJsonObject { put("title", title) }

        fun observation(
            generation: Long,
            vararg windows: AccessibilityWindowSnapshot,
        ) = AccessibilityObservation(
            generation = generation,
            observedAtMs = generation,
            displayRotation = 0,
            densityDpi = 320,
            windows = windows.toList(),
            elements = emptyList(),
            truncated = false,
        )

        fun app(
            id: Int = 10,
            packageName: String = PACKAGE,
            title: String = "Fixture",
            displayId: Int = 0,
            active: Boolean,
            focused: Boolean = active,
            bounds: TargetBounds = DISPLAY_BOUNDS,
            pictureInPicture: Boolean = false,
        ) = AccessibilityWindowSnapshot(
            id = id,
            displayId = displayId,
            layer = 1,
            type = "application",
            title = title,
            packageName = packageName,
            active = active,
            focused = focused,
            bounds = bounds,
            pictureInPicture = pictureInPicture,
        )

        fun blindWindow() = AccessibilityWindowSnapshot(
            id = 30,
            displayId = 0,
            layer = 2,
            type = "system",
            title = "Secure",
            packageName = null,
            active = false,
            focused = false,
            bounds = TargetBounds(0, 0, 1000, 100),
            contentAccessible = false,
        )

        fun splitDivider() = AccessibilityWindowSnapshot(
            id = 40,
            displayId = 0,
            layer = 3,
            type = "split_screen_divider",
            title = null,
            packageName = null,
            active = false,
            focused = false,
            bounds = TargetBounds(0, 890, 1000, 910),
        )

        val DISPLAY_BOUNDS = TargetBounds(0, 0, 1000, 1800)
    }
}

private fun JsonObject.string(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.boolean(name: String): Boolean = getValue(name).jsonPrimitive.content.toBoolean()
