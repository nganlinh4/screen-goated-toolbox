package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityRoute
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlCapabilityContract
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderDefinition
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouter
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.InputInjectionEvidence
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlResultEnvelope
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.RequiredUserStep
import dev.screengoated.toolbox.mobile.phonecontrol.result.ResultScope
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolRegistry
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlAuthorityMatrixTest {
    @Test
    fun `stable capability and catalog policy match the shared authority fixture`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val catalog = fixture.root.getValue("catalog").jsonObject

        assertEquals(11L, fixture.root.getValue("schemaVersion").jsonPrimitive.long)
        assertEquals("phone-control", fixture.root.getValue("feature").jsonPrimitive.content)
        val distribution = fixture.root.getValue("distribution").jsonObject
        assertEquals(
            listOf("play", "full"),
            distribution.getValue("agentFlavors").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals("identical", distribution.getValue("behavior").jsonPrimitive.content)
        assertTrue(distribution.getValue("catalogAndRuntimeMustMatch").jsonPrimitive.boolean)
        assertTrue(
            distribution.getValue("largeOfflineAssetDeliveryMayDiffer").jsonPrimitive.boolean,
        )
        assertTrue(
            distribution.getValue("largeOfflineAssetIdentityMustMatch").jsonPrimitive.boolean,
        )
        assertEquals(
            "feature_asr_ort",
            distribution.getValue("playDetectorModule").jsonPrimitive.content,
        )
        assertFalse(distribution.getValue("playDetectorNetworkFallback").jsonPrimitive.boolean)
        assertEquals(
            fixture.capabilityStates,
            CapabilityState.entries.map(CapabilityState::wireName),
        )
        assertEquals(
            catalog.getValue("normalTurnPolicy").jsonPrimitive.content,
            PhoneControlCapabilityContract.NORMAL_TURN_POLICY,
        )
        assertEquals(
            catalog.getValue("unavailableResultCode").jsonPrimitive.content,
            PhoneControlCapabilityContract.UNAVAILABLE_RESULT_CODE,
        )
        assertEquals(
            catalog.getValue("dynamicHiding").jsonPrimitive.boolean,
            PhoneControlCapabilityContract.DYNAMIC_HIDING,
        )
        assertEquals(
            catalog.getValue("phraseOrLanguageGates").jsonPrimitive.boolean,
            PhoneControlCapabilityContract.PHRASE_OR_LANGUAGE_GATES,
        )
        assertEquals(
            catalog.getValue("silentToolReroutes").jsonPrimitive.boolean,
            PhoneControlCapabilityContract.SILENT_TOOL_REROUTES,
        )
        val targetAuthority = fixture.root.getValue("targetEffectAuthority").jsonObject
        assertEquals(
            listOf("routine", "consequential", "os_owned_user_step"),
            targetAuthority.getValue("states").jsonArray.map { it.jsonPrimitive.content },
        )
        assertEquals(
            "platform_structure_only",
            targetAuthority.getValue("derivation").jsonPrimitive.content,
        )
        assertFalse(
            targetAuthority.getValue("labelsOrUserPhrasesMayAssignAuthority")
                .jsonPrimitive.boolean,
        )
        assertEquals(
            "before_every_platform_dispatch",
            targetAuthority.getValue("enforcementBoundary").jsonPrimitive.content,
        )
        assertFalse(
            targetAuthority.getValue("alternateToolMayWeakenAuthority").jsonPrimitive.boolean,
        )
        assertTrue(
            targetAuthority.getValue("coveredDispatches").jsonArray
                .map { it.jsonPrimitive.content }
                .containsAll(setOf("global_action", "command_execution")),
        )
        assertFalse(
            targetAuthority.getValue("commandTextMayAssignOrClearAuthority")
                .jsonPrimitive.boolean,
        )
        assertTrue(
            targetAuthority.getValue("platformPendingConfirmationUsesOpaqueSession")
                .jsonPrimitive.boolean,
        )
        assertTrue(
            targetAuthority.getValue("activeOsOwnedStepBlocksElevatedCommandDispatch")
                .jsonPrimitive.boolean,
        )
        assertTrue(targetAuthority.getValue("consequentialRequiresConfirm").jsonPrimitive.boolean)
        assertTrue(
            targetAuthority.getValue("osOwnedConfirmationIsAlwaysUserStep")
                .jsonPrimitive.boolean,
        )
        val selfTest = fixture.root.getValue("setupSelfTest").jsonObject
        assertEquals(
            listOf("play", "full"),
            selfTest.getValue("flavors").jsonArray.map { it.jsonPrimitive.content },
        )
        assertEquals(
            "non_catalog_local_accessibility_seam",
            selfTest.getValue("dispatchBoundary").jsonPrimitive.content,
        )
        assertEquals(
            "accessibility_focus_then_clear",
            selfTest.getValue("allowedMutation").jsonPrimitive.content,
        )
        assertFalse(selfTest.getValue("labelsOrCoordinatesMaySelectTarget").jsonPrimitive.boolean)
        assertFalse(selfTest.getValue("agentTargetingEnabled").jsonPrimitive.boolean)
        assertTrue(
            selfTest.getValue("productionControllerTargetingRemainsBlocked")
                .jsonPrimitive.boolean,
        )
        assertTrue(selfTest.getValue("exactObservationLeaseRequired").jsonPrimitive.boolean)
        assertTrue(selfTest.getValue("osOwnedStepRemainsBlocked").jsonPrimitive.boolean)
        assertTrue(
            selfTest.getValue("successRequiresVerifiedTransitionAndRestore")
                .jsonPrimitive.boolean,
        )
        val privacy = fixture.root.getValue("privacyBoundary").jsonObject
        assertEquals(
            "node_isPassword",
            privacy.getValue("accessibilityPasswordSignal").jsonPrimitive.content,
        )
        assertFalse(
            privacy.getValue("protectedNodeTextOrValueMayLeaveProvider").jsonPrimitive.boolean,
        )
        assertFalse(
            privacy.getValue("protectedNodeSecretDerivedHashAllowed").jsonPrimitive.boolean,
        )
        assertEquals(
            listOf("text", "value", "content_description", "hint", "state_description"),
            privacy.getValue("protectedNodeDroppedFields").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals(
            listOf("role", "view_id", "actions", "bounds"),
            privacy.getValue("protectedNodeRetainedFields").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals(
            "privacy=protected",
            privacy.getValue("protectedNodeMarker").jsonPrimitive.content,
        )
        assertFalse(
            privacy.getValue("browserExtractPageInlinePreview").jsonPrimitive.boolean,
        )
        assertTrue(
            privacy.getValue("browserArtifactUsesProtectedSafeCapture").jsonPrimitive.boolean,
        )
    }

    @Test
    fun `fixture provider and route topology is accepted without authority inference`() {
        val fixture = PhoneControlAuthorityFixture.load()
        ProviderRouter(fixture.providers, fixture.routes)

        val providerIds = fixture.providers.map(ProviderDefinition::id).toSet()
        assertTrue(fixture.routes.isNotEmpty())
        fixture.routes.forEach { route ->
            assertTrue(route.providerIds.all(providerIds::contains))
            assertEquals(route.providerIds.distinct(), route.providerIds)
        }
        PhoneControlToolRegistry.specs.forEach { spec ->
            assertTrue(spec.dependencyProviderIds.all(providerIds::contains))
        }
        val selection = fixture.root.getValue("providerSelection").jsonObject
        assertEquals(
            "narrowest_ready_provider_with_full_requested_semantics",
            selection.getValue("rule").jsonPrimitive.content,
        )
        assertTrue(selection.getValue("mustReportProvider").jsonPrimitive.boolean)
        assertEquals(
            "exact_tool_provider_subset_of_exact_capability_route",
            selection.getValue("dispatchPlan").jsonPrimitive.content,
        )
        assertFalse(selection.getValue("registrySnapshotPreGate").jsonPrimitive.boolean)
        assertEquals(
            "provider_specific_handler",
            selection.getValue("liveSelectionOwner").jsonPrimitive.content,
        )
        assertTrue(
            selection.getValue("successfulOrEffectfulReceiptMustMatchPlan")
                .jsonPrimitive.boolean,
        )
        assertEquals(
            "ready",
            selection.getValue("successfulReceiptState").jsonPrimitive.content,
        )
        assertTrue(
            selection.getValue("effectfulFailurePreservesReportedProviderState")
                .jsonPrimitive.boolean,
        )
        assertTrue(
            selection.getValue("toolContractBypassRequiresProvenNoEffectNonmutatingFailure")
                .jsonPrimitive.boolean,
        )
        assertTrue(selection.getValue("primaryReceiptProviderOnly").jsonPrimitive.boolean)
        assertTrue(
            selection.getValue("dependencyProviderFieldsAreEvidenceNotRouteSelection")
                .jsonPrimitive.boolean,
        )
        assertTrue(
            selection.getValue("dependencyFailureMustBeDeclaredPerTool")
                .jsonPrimitive.boolean,
        )
        assertEquals(
            "provider_role",
            selection.getValue("dependencyReceiptRoleField").jsonPrimitive.content,
        )
        assertEquals(
            "provider_contract_failure",
            selection.getValue("providerContractFailureCode").jsonPrimitive.content,
        )
        assertEquals(
            "internal",
            selection.getValue("providerContractFailureClass").jsonPrimitive.content,
        )
        assertTrue(
            selection.getValue("noEffectDependencyFailureMayReportAttemptedProvider")
                .jsonPrimitive.boolean,
        )
        assertTrue(
            selection.getValue("unattributedProviderFailureMustNotGuessProvider")
                .jsonPrimitive.boolean,
        )
        assertTrue(
            selection.getValue("strongerAuthorityDoesNotOverrideBetterEvidence")
                .jsonPrimitive.boolean,
        )
        val visual = fixture.root.getValue("surfaceSemantics")
            .jsonObject
            .getValue("visualObservation")
            .jsonObject
        assertEquals(
            "same_tool_dual_semantic_selection_one_frame_detector_refresh_" +
                "dual_crosshair_verification_exact_surface_lease",
            visual.getValue("dragTarget").jsonPrimitive.content,
        )
        val gesture = fixture.root.getValue("surfaceSemantics")
            .jsonObject
            .getValue("gestureDispatch")
            .jsonObject
        assertEquals(
            "proven_no_effect",
            gesture.getValue("rejectedBeforePlatformAcceptance").jsonPrimitive.content,
        )
        assertEquals(
            "effect_may_have_occurred",
            gesture.getValue("acceptedThenCallbackCancelled").jsonPrimitive.content,
        )
        assertEquals(
            "effect_may_have_occurred",
            gesture.getValue("acceptedThenCompleted").jsonPrimitive.content,
        )
        assertEquals(
            "effect_may_have_occurred",
            gesture.getValue("acceptedThenCoroutineCancelled").jsonPrimitive.content,
        )
        assertTrue(gesture.getValue("acceptedGestureInvalidatesSnapshot").jsonPrimitive.boolean)
        val surfaces = fixture.root.getValue("surfaceSemantics").jsonObject
        val ownership = surfaces.getValue("ownership").jsonObject
        assertEquals(
            "accessibility_overlay_or_same_package_non_application_window",
            ownership.getValue("controllerOwnedScope").jsonPrimitive.content,
        )
        assertEquals(
            "ordinary_targetable_surface",
            ownership.getValue("samePackageApplicationWindow").jsonPrimitive.content,
        )
        assertTrue(
            ownership.getValue("ordinaryToolsBlockControllerOverlay").jsonPrimitive.boolean,
        )
        val navigation = surfaces.getValue("systemNavigationKeys").jsonObject
        assertEquals("key_combination", navigation.getValue("tool").jsonPrimitive.content)
        assertEquals("accessibility", navigation.getValue("baselineProvider").jsonPrimitive.content)
        assertFalse(navigation.getValue("focusedEditorRequired").jsonPrimitive.boolean)
        assertTrue(
            navigation.getValue("exactForegroundSurfaceLeaseRequired").jsonPrimitive.boolean,
        )
        assertEquals(
            "current_non_controller_platform_window",
            navigation.getValue("foregroundSurfaceScope").jsonPrimitive.content,
        )
        assertFalse(navigation.getValue("pointerGeometryRequired").jsonPrimitive.boolean)
        assertFalse(navigation.getValue("inactiveHigherWindowBlocksDispatch").jsonPrimitive.boolean)
        assertTrue(
            navigation.getValue("activeOsOwnedUserStepBlocksDispatch").jsonPrimitive.boolean,
        )
        assertEquals(
            listOf("back", "home", "recents", "notifications", "quick_settings"),
            navigation.getValue("keys").jsonArray.map { it.jsonPrimitive.content },
        )
        val recovery = surfaces.getValue("staleRecovery").jsonObject
        assertFalse(recovery.getValue("automaticTargetRebinding").jsonPrimitive.boolean)
        assertTrue(
            recovery.getValue("provenNoEffectReceiptAttachesFreshObservation")
                .jsonPrimitive.boolean,
        )
        assertTrue(
            recovery.getValue("attachedObservationContainsCurrentSurfaceTargets")
                .jsonPrimitive.boolean,
        )
        val invalidation = surfaces.getValue("observationInvalidation").jsonObject
        assertFalse(
            invalidation.getValue("backgroundVisualCaptureMayReplaceActionLeases")
                .jsonPrimitive.boolean,
        )
        assertFalse(
            invalidation.getValue("semanticOnlyAccessibilityChurnInvalidatesImmediately")
                .jsonPrimitive.boolean,
        )
        assertTrue(
            invalidation.getValue("everyMutationRevalidatesLiveTargetFingerprint")
                .jsonPrimitive.boolean,
        )
    }

    @Test
    fun `target and result wire envelopes cover every fixture field`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val target = PhoneControlTargetIdentity(
            snapshotGeneration = 31,
            displayId = 0,
            windowId = 7,
            packageOrSurface = "dev.example",
            nodeOrDocumentIdentity = "root/2/1",
            bounds = TargetBounds(10, 20, 110, 220),
            observationTimestampMs = 9_000,
        )
        val targetJson = target.toWireJson()
        val minimumTargetFields = fixture.root
            .getValue("targetIdentity")
            .jsonObject
            .getValue("minimumFields")
            .jsonArray
            .map { it.jsonPrimitive.content }
        assertTrue(targetJson.keys.containsAll(minimumTargetFields))

        val result = PhoneControlResultEnvelope(
            code = "stale_target",
            capability = "ui.pointer_action",
            requestedTool = "click_target",
            turnId = 12,
            jobId = "job-12",
            provider = "accessibility",
            providerState = CapabilityState.READY,
            observationGeneration = 31,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = true,
            retryable = true,
            requiredUserStep = RequiredUserStep("observe_again"),
            freshObservationRequired = true,
            scope = ResultScope(displayId = 0, userId = 10, surface = "window:7"),
            target = target,
        ).toWireJson()
        val typedResult = fixture.root.getValue("typedResult").jsonObject
        val required = typedResult.getValue("requiredFields").jsonArray
            .map { it.jsonPrimitive.content }
        val conditional = typedResult.getValue("conditionalFields").jsonArray
            .map { it.jsonPrimitive.content }
        assertTrue(result.keys.containsAll(required))
        assertTrue(result.keys.containsAll(conditional))
        assertEquals(false, result.getValue("effect_may_have_occurred").jsonPrimitive.boolean)
        assertEquals(false, result.getValue("effect_verified").jsonPrimitive.boolean)
        assertEquals(false, result.getValue("executed").jsonPrimitive.boolean)
        assertEquals("click_target", result.getValue("requested_tool").jsonPrimitive.content)
    }

    @Test
    fun `effect certainty preserves verified maybe no-effect and unknown states`() {
        assertEquals(
            EffectCertainty.VERIFIED,
            EffectCertainty.fromSignals(
                effectVerified = true,
                effectMayHaveOccurred = false,
            ),
        )
        assertEquals(
            EffectCertainty.MAY_HAVE_OCCURRED,
            EffectCertainty.fromSignals(dispatchOk = true, executed = false),
        )
        assertEquals(
            EffectCertainty.PROVEN_NO_EFFECT,
            EffectCertainty.fromSignals(
                inputInjection = InputInjectionEvidence(
                    requested = 3,
                    inserted = 0,
                    fullyInserted = false,
                ),
            ),
        )
        assertEquals(EffectCertainty.UNKNOWN, EffectCertainty.fromSignals())
        assertEquals(
            EffectCertainty.MAY_HAVE_OCCURRED,
            EffectCertainty.UNKNOWN.afterDispatch(mutating = true),
        )
        assertEquals(
            EffectCertainty.UNKNOWN,
            EffectCertainty.UNKNOWN.afterDispatch(mutating = false),
        )
        assertNull(EffectCertainty.MAY_HAVE_OCCURRED.executed)
        assertFalse(EffectCertainty.PROVEN_NO_EFFECT.effectVerified)
        assertTrue(EffectCertainty.VERIFIED.effectVerified)
    }
}

internal data class PhoneControlAuthorityFixtureData(
    val root: JsonObject,
    val capabilityStates: List<String>,
    val providers: List<ProviderDefinition>,
    val routes: List<CapabilityRoute>,
)

internal object PhoneControlAuthorityFixture {
    private const val FIXTURE_PATH = "parity-fixtures/phone-control/authority-matrix.json"

    fun load(): PhoneControlAuthorityFixtureData {
        val root = Json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText()).jsonObject
        val providers = root.getValue("providers").jsonArray.map { element ->
            val provider = element.jsonObject
            ProviderDefinition(
                id = provider.getValue("id").jsonPrimitive.content,
                authority = provider.getValue("authority").jsonPrimitive.content,
                optional = provider.getValue("optional").jsonPrimitive.boolean,
            )
        }
        val routes = root.getValue("routes").jsonArray.map { element ->
            val route = element.jsonObject
            CapabilityRoute(
                capability = route.getValue("capability").jsonPrimitive.content,
                providerIds = route.getValue("providers").jsonArray.map {
                    it.jsonPrimitive.content
                },
            )
        }
        return PhoneControlAuthorityFixtureData(
            root = root,
            capabilityStates = root.getValue("capabilityStates").jsonArray.map {
                it.jsonPrimitive.content
            },
            providers = providers,
            routes = routes,
        )
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }
}
