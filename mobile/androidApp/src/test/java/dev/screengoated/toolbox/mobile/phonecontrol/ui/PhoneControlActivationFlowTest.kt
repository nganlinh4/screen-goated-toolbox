package dev.screengoated.toolbox.mobile.phonecontrol.ui

import java.io.File
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuBridgeCondition
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuBridgeProbe
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlActivationFlowTest {
    @Test
    fun `activation reducer and launcher contract match the shared fixture`() {
        val fixture = Json.parseToJsonElement(fixtureFile().readText()).jsonObject
        assertEquals(13L, fixture.getValue("schemaVersion").jsonPrimitive.long)
        assertEquals(
            PhoneControlActivationStep.entries.map(PhoneControlActivationStep::wireName),
            fixture.getValue("requiredOrder").jsonArray.map { it.jsonPrimitive.content },
        )

        val invariants = fixture.getValue("invariants").jsonObject
        assertEquals("apps_card", invariants.string("launcherSurface"))
        assertEquals("adjacent_to_live_translate", invariants.string("launcherPlacement"))
        assertFalse(invariants.boolean("innerSetupScreen"))
        assertEquals("existing_settings_section_with_toast", invariants.string("apiKeySurface"))
        assertTrue(invariants.boolean("opensGeneralSettingsForApiKey"))
        val languageContract = invariants.getValue("languageContract").jsonObject
        assertEquals(
            listOf("default", "ko", "vi"),
            languageContract.getValue("explicitUiLocales").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals(
            "android_default_resource_fallback",
            languageContract.string("otherUiLocales"),
        )
        assertEquals(
            "provider_detected_without_app_locale_override",
            languageContract.string("liveLanguage"),
        )
        assertEquals(
            listOf("input", "output"),
            languageContract.getValue("transcription").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertFalse(languageContract.boolean("languageSpecificRouting"))
        assertFalse(invariants.boolean("userFacingSelfTest"))
        assertTrue(invariants.boolean("oneExternalUserStepAtATime"))
        assertTrue(invariants.boolean("reprobeAfterReturn"))
        val accessibility = invariants.getValue("accessibilityReadiness").jsonObject
        assertEquals(
            listOf("configured_service", "live_service_binding"),
            accessibility.getValue("requiredEvidence").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertFalse(accessibility.boolean("configuredSettingAloneIsReady"))
        assertEquals(
            "bounded_reconnect_wait_then_android_settings_user_step",
            accessibility.string("configuredButUnbound"),
        )
        assertFalse(accessibility.boolean("serviceBoundAfterSettingRemovalIsReady"))
        val projection = invariants.getValue("mediaProjection").jsonObject
        assertTrue(projection.boolean("requiredForEverySession"))
        assertEquals("whole_default_display", projection.string("scope"))
        assertEquals(
            "single_session_single_consumption",
            projection.string("grantLifetime"),
        )
        assertTrue(projection.boolean("serviceStartsBeforeProjectionConsumption"))
        assertTrue(projection.boolean("runtimeStartsAfterVirtualDisplay"))
        assertEquals("stop_phone_control", projection.string("revocation"))
        assertEquals("stop_without_service", projection.string("denial"))
        val protectedCheckpoint = projection.getValue("protectedUserCheckpoint").jsonObject
        assertEquals(
            "complete_before_handoff",
            protectedCheckpoint.string("agentNavigation"),
        )
        assertEquals(
            "seal_visual_evidence_drain_then_release",
            protectedCheckpoint.string("capture"),
        )
        assertEquals(
            "keep_socket_microphone_audio_orb_and_conversation",
            protectedCheckpoint.string("runtime"),
        )
        assertEquals(
            listOf("idle", "listening"),
            protectedCheckpoint.getValue("settleWhen").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals(
            "public_persistent_notification",
            protectedCheckpoint.string("guidance"),
        )
        assertEquals(
            "explicit_notification_action",
            protectedCheckpoint.string("cancel"),
        )
        val cancelOutcome = protectedCheckpoint.getValue("cancelOutcome").jsonObject
        assertEquals("standard", cancelOutcome.string("authority"))
        assertEquals("cancel", cancelOutcome.string("adapter"))
        assertEquals(
            "remain_live_and_visually_sealed",
            cancelOutcome.string("runtime"),
        )
        assertEquals("fresh_media_projection", cancelOutcome.string("nextStep"))
        assertEquals(
            "attach_fresh_media_projection_to_existing_runtime",
            protectedCheckpoint.string("resume"),
        )
        assertFalse(protectedCheckpoint.boolean("providerSetupWhileSealed"))
        assertEquals(
            "blocked_until_fresh_projection_attached",
            protectedCheckpoint.string("modelVisualEvidence"),
        )
        assertEquals(
            "ephemeral_structural_provider_adapter_only",
            protectedCheckpoint.string("localSecretTransfer"),
        )
        assertFalse(protectedCheckpoint.boolean("modelSecretAccess"))
        assertFalse(protectedCheckpoint.boolean("secretPersistence"))
        assertEquals(
            "typed_user_step_without_false_success",
            protectedCheckpoint.string("relayFailure"),
        )
        val relayContinuation = protectedCheckpoint.getValue("relayContinuation").jsonObject
        assertEquals(
            "resume_selected_setup_after_fresh_projection",
            relayContinuation.string("completed"),
        )
        listOf("needsUserStep", "failed").forEach { outcome ->
            assertEquals(
                "hold_selected_without_republishing_identical_goal",
                relayContinuation.string(outcome),
            )
        }
        assertEquals(
            "explicit_user_action_or_fresh_capability_evidence",
            relayContinuation.string("retry"),
        )
        assertEquals(
            "stop_phone_control",
            protectedCheckpoint.string("unexpectedProjectionLoss"),
        )
        val navigation = invariants.getValue("settingsNavigation").jsonObject
        assertEquals(
            "runtime_app_label_on_resolved_settings_package",
            navigation.string("targetIdentity"),
        )
        assertTrue(navigation.boolean("mayScroll"))
        assertTrue(navigation.boolean("mayOpenAppRow"))
        assertFalse(navigation.boolean("mayTogglePermission"))
        val activePlatformSession = navigation.getValue("activePlatformSession").jsonObject
        assertEquals(
            "routine_navigation",
            activePlatformSession.string("fullScreenSetupSurface"),
        )
        assertEquals(
            "os_owned_user_step",
            activePlatformSession.string("liveModalAboveApplication"),
        )
        assertEquals(
            "back_only_while_resolved_settings_is_foreground",
            navigation.string("returnAfterGrant"),
        )
        assertEquals("stop_without_effect", navigation.string("ambiguousTarget"))
        assertEquals("orb", invariants.string("optionalPowerPromptOwner"))
        assertEquals(
            listOf(
                "coordinator_open",
                "step_selected",
                "user_step_opened",
                "user_step_returned",
                "settings_app_row_opened",
                "settings_grant_observed",
                "settings_returned",
                "service_start_accepted",
                "runtime_terminal",
            ),
            invariants.getValue("diagnosticMilestones").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals(
            PhoneControlPowerChoice.entries.map(PhoneControlPowerChoice::wireName),
            invariants.getValue("optionalPowerChoices").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        val powerSelection = invariants.getValue("powerAuthoritySelection").jsonObject
        val powerPresentation = powerSelection.getValue("presentation").jsonObject
        assertEquals("compact_orb_card", powerPresentation.string("surface"))
        assertFalse(powerPresentation.boolean("explanatoryParagraph"))
        assertEquals("sgt_adb", powerPresentation.string("recommendedNonRoot"))
        assertEquals("star_icon", powerPresentation.string("recommendedMarker"))
        assertEquals(
            "compact_secondary_action_when_paired",
            powerPresentation.string("forgetPairing"),
        )
        assertTrue(powerSelection.boolean("persistBeforeSetup"))
        assertTrue(powerSelection.getValue("standard").jsonArray.isEmpty())
        assertEquals(
            listOf("sgt_adb_bridge"),
            powerSelection.getValue("sgt_adb").jsonArray.map { it.jsonPrimitive.content },
        )
        assertEquals(
            listOf("shizuku_shell"),
            powerSelection.getValue("shizuku").jsonArray.map { it.jsonPrimitive.content },
        )
        assertEquals(
            listOf("root_bridge"),
            powerSelection.getValue("root").jsonArray.map { it.jsonPrimitive.content },
        )
        assertEquals(
            "unavailable",
            powerSelection.string("unselectedElevatedProviderState"),
        )
        assertTrue(powerSelection.boolean("changingChoiceCancelsPendingSetup"))
        val protectedChange = powerSelection
            .getValue("changeDuringProtectedCheckpoint")
            .jsonObject
        assertEquals("cancel", protectedChange.string("oldAdapter"))
        assertEquals(
            "remain_live_and_visually_sealed",
            protectedChange.string("runtime"),
        )
        assertEquals("fresh_media_projection", protectedChange.string("nextStep"))
        assertEquals(
            "start_only_after_projection_attach",
            protectedChange.string("selectedSetup"),
        )
        assertFalse(protectedChange.boolean("automationWhileToolsBlocked"))
        PhoneControlPowerChoice.entries.forEach { choice ->
            assertEquals(
                PhoneControlPowerSelectionRoute.RESUME_CAPTURE,
                phoneControlPowerSelectionRoute(choice, protectedCheckpointActive = true),
            )
        }
        assertEquals(
            PhoneControlPowerSelectionRoute.NONE,
            phoneControlPowerSelectionRoute(
                PhoneControlPowerChoice.STANDARD,
                protectedCheckpointActive = false,
            ),
        )
        PhoneControlPowerChoice.entries
            .filter { it.elevatedProviderId != null }
            .forEach { choice ->
                assertEquals(
                    PhoneControlPowerSelectionRoute.SETUP,
                    phoneControlPowerSelectionRoute(
                        choice,
                        protectedCheckpointActive = false,
                    ),
                )
            }
        val reconciliationWait = invariants.getValue("reconciliationWait").jsonObject
        assertTrue(reconciliationWait.boolean("mutationGate"))
        assertEquals(
            "preserve_current_turn_phase",
            reconciliationWait.string("visibleState"),
        )
        assertEquals(
            "bounded_screen_capture_failure_only",
            reconciliationWait.string("failureEscalation"),
        )
        PhoneControlPowerChoice.entries.forEach { choice ->
            val expected = powerSelection.getValue(choice.wireName).jsonArray
                .map { it.jsonPrimitive.content }
            assertEquals(expected.singleOrNull(), choice.elevatedProviderId)
        }
        val shizuku = invariants.getValue("shizukuSetup").jsonObject
        assertTrue(shizuku.boolean("feedbackBeforeExternalStep"))
        assertEquals(
            listOf("orb_caption", "ongoing_notification"),
            shizuku.getValue("durableFeedbackSurfaces").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        val guidancePresentation = shizuku.getValue("guidancePresentation").jsonObject
        assertEquals(
            "compact_non_obscuring_status",
            guidancePresentation.string("orbCaption"),
        )
        assertEquals(
            "full_persistent_instruction",
            guidancePresentation.string("ongoingNotification"),
        )
        assertEquals("structural_probe_state", shizuku.string("plannerInput"))
        val liveAutomation = shizuku.getValue("liveAutomation").jsonObject
        assertEquals("user_power_authority_selection", liveAutomation.string("trigger"))
        assertEquals(
            "one_bounded_goal_when_idle",
            liveAutomation.string("turnBoundary"),
        )
        assertEquals("normal_full_catalog", liveAutomation.string("catalog"))
        assertEquals(
            "normal_semantic_and_vision_tools",
            liveAutomation.string("navigation"),
        )
        assertFalse(liveAutomation.boolean("providerSpecificClickScript"))
        assertEquals("remain_pending", liveAutomation.string("busyTurn"))
        assertEquals(
            "expose_surface_without_entry_or_approval",
            liveAutomation.string("checkpointNavigation"),
        )
        assertEquals(
            "finish_seal_visuals_then_release_capture",
            liveAutomation.string("screenShareProtectedCheckpoint"),
        )
        assertEquals(
            "local_ephemeral_adapter_without_model_visibility",
            liveAutomation.string("privateRelay"),
        )
        assertEquals("user_step", liveAutomation.string("systemOwnedConfirmation"))
        assertEquals("until_ready_or_authority_changed", shizuku.string("lifetime"))
        assertEquals("complete", shizuku.string("ready"))
        assertEquals("request_permission", shizuku.string("binderReadyPermissionMissing"))
        assertEquals("open_manager", shizuku.string("installedServiceStoppedOrGrantRevoked"))
        assertEquals(
            "open_store_with_official_download_fallback",
            shizuku.string("packageMissingOrApiUnsupported"),
        )
        assertEquals(
            "on_package_change_external_return_or_binder_event",
            shizuku.string("reprobe"),
        )
        assertEquals("open_manager_without_reselection", shizuku.string("packageInstalled"))
        assertEquals(
            "remain_selected_pending_without_reopening",
            shizuku.string("unchangedExternalState"),
        )
        assertEquals("user_step", shizuku.string("androidPlayInstallConfirmation"))
        assertEquals("user_step", shizuku.string("androidOwnedPairingAndTrust"))
        assertEquals(
            "sealed_local_adapter_or_user_step",
            shizuku.string("oneTimePairingCodeTransfer"),
        )
        val shizukuCases = shizuku.getValue("cases").jsonArray
        assertEquals(ShizukuBridgeCondition.entries.size, shizukuCases.size)
        shizukuCases.forEach { element ->
            val case = element.jsonObject
            val condition = ShizukuBridgeCondition.entries.single {
                it.wireName == case.string("condition")
            }
            val actual = nextPhoneControlShizukuSetupAction(
                ShizukuBridgeProbe(
                    state = CapabilityState.NEEDS_USER_STEP,
                    condition = condition,
                ),
            )
            assertEquals(case.string("expect"), actual.wireName)
        }
        val firstPartyAdb = invariants.getValue("firstPartyAdbSetup").jsonObject
        assertEquals(
            "serialize_then_forget_abandoned_client_key_after_terminal_return",
            firstPartyAdb.string("cancelDuringPairing"),
        )
        assertEquals(
            "persist_pairing_then_bounded_reconnect_without_code_reentry",
            firstPartyAdb.string("pairingEstablishedBeforeConnect"),
        )
        assertEquals(
            "paired_client_key_and_persisted_adb_mdns_identity_family",
            firstPartyAdb.string("reconnect"),
        )

        fixture.getValue("cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val snapshot = case.getValue("snapshot").jsonObject
            val actual = nextPhoneControlActivationStep(
                PhoneControlActivationSnapshot(
                    apiKeyReady = snapshot.boolean("apiKey"),
                    microphoneReady = snapshot.boolean("microphone"),
                    notificationsReady = snapshot.boolean("notifications"),
                    notificationPrompted = snapshot.boolean("notificationPrompted"),
                    accessibilityReady = snapshot.boolean("accessibilityReady"),
                    overlayReady = snapshot.boolean("overlay"),
                    mediaProjectionReady = snapshot.boolean("mediaProjection"),
                ),
            )
            assertEquals(case.string("name"), case.string("expect"), actual.wireName)
        }
    }

    @Test
    fun `accessibility readiness requires configuration and a live binding`() {
        assertEquals(
            PhoneControlAccessibilityState.DISABLED,
            phoneControlAccessibilityState(configured = false, serviceBound = false),
        )
        assertEquals(
            PhoneControlAccessibilityState.DISABLED,
            phoneControlAccessibilityState(configured = false, serviceBound = true),
        )
        assertEquals(
            PhoneControlAccessibilityState.RECONNECTING,
            phoneControlAccessibilityState(configured = true, serviceBound = false),
        )
        assertEquals(
            PhoneControlAccessibilityState.READY,
            phoneControlAccessibilityState(configured = true, serviceBound = true),
        )
    }

    @Test
    fun `Shizuku setup advances on state change without repeating one external step`() {
        val missing = PhoneControlShizukuSetupAttempt(
            ShizukuBridgeCondition.PACKAGE_MISSING,
            PhoneControlShizukuSetupAction.OPEN_STORE,
        )
        val installed = PhoneControlShizukuSetupAttempt(
            ShizukuBridgeCondition.SERVICE_STOPPED,
            PhoneControlShizukuSetupAction.OPEN_MANAGER,
        )

        assertEquals(
            PhoneControlShizukuRepeatDisposition.DISPATCH,
            phoneControlShizukuRepeatDisposition(missing, previous = null, stepActive = false),
        )
        assertEquals(
            PhoneControlShizukuRepeatDisposition.WAIT_FOR_EVENT,
            phoneControlShizukuRepeatDisposition(missing, missing, stepActive = true),
        )
        assertEquals(
            PhoneControlShizukuRepeatDisposition.LEAVE_SELECTED_PENDING,
            phoneControlShizukuRepeatDisposition(missing, missing, stepActive = false),
        )
        assertEquals(
            PhoneControlShizukuRepeatDisposition.DISPATCH,
            phoneControlShizukuRepeatDisposition(installed, missing, stepActive = false),
        )
    }

    private fun fixtureFile(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, FIXTURE_PATH) }
            .firstOrNull(File::isFile)
            ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private fun kotlinx.serialization.json.JsonObject.boolean(field: String): Boolean =
        getValue(field).jsonPrimitive.boolean

    private fun kotlinx.serialization.json.JsonObject.string(field: String): String =
        getValue(field).jsonPrimitive.content

    private companion object {
        const val FIXTURE_PATH = "parity-fixtures/phone-control/activation-flow.json"
    }
}
