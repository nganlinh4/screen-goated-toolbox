package dev.screengoated.toolbox.mobile.phonecontrol.ui

import java.io.File
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
        assertEquals(28L, fixture.getValue("schemaVersion").jsonPrimitive.long)
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
        assertEquals("in_app_language_setting", languageContract.string("uiLocaleSource"))
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
        val portability = invariants.getValue("devicePortability").jsonObject
        assertFalse(portability.boolean("identityBasedBranching"))
        assertFalse(portability.boolean("emulatorSpecificRuntimePath"))
        assertEquals(
            "live_display_window_insets_density_and_rotation",
            portability.string("geometrySource"),
        )
        assertEquals(
            "runtime_probe_and_android_api_contract",
            portability.string("capabilitySource"),
        )
        assertEquals("typed_capability_result", portability.string("unsupportedOutcome"))
        assertFalse(invariants.boolean("userFacingSelfTest"))
        assertTrue(invariants.boolean("oneExternalUserStepAtATime"))
        assertTrue(invariants.boolean("reprobeAfterReturn"))
        val internalAutomation = invariants.getValue("internalSetupAutomation").jsonObject
        assertEquals("normal_full_catalog", internalAutomation.string("catalog"))
        assertEquals("silent", internalAutomation.string("presentation"))
        assertTrue(internalAutomation.boolean("ownershipBeforeSend"))
        val setupSingleFlight = internalAutomation.getValue("singleFlight").jsonObject
        assertEquals("selected_provider", setupSingleFlight.string("scope"))
        assertEquals(
            "coalesce_guidance_and_strengthen_handoff_on_original_goal",
            setupSingleFlight.string("sameProviderReentry"),
        )
        assertEquals("cannot_inherit_active_goal", setupSingleFlight.string("differentProvider"))
        assertEquals("original_goal_id", setupSingleFlight.string("completionOwner"))
        assertEquals("discard", internalAutomation.string("assistantAudio"))
        assertEquals("discard", internalAutomation.string("assistantCaption"))
        assertEquals(
            "preserve_setup_owned_caption_state_and_icon",
            internalAutomation.string("orbPresentation"),
        )
        assertEquals(
            "exclude_entire_internal_turn",
            internalAutomation.string("memory"),
        )
        assertEquals(
            "localized_structural_state",
            internalAutomation.string("completionFeedback"),
        )
        assertEquals(
            "restore_normal_conversation_before_new_turn",
            internalAutomation.string("userInterruption"),
        )
        val setupBoundary = invariants.getValue("setupSessionBoundary").jsonObject
        assertEquals(
            "one_localized_app_owned_voice_message",
            setupBoundary.string("startAnnouncement"),
        )
        assertEquals(
            "discard_locally_while_setup_active",
            setupBoundary.string("microphoneSamples"),
        )
        assertFalse(setupBoundary.boolean("modelMicrophoneInput"))
        assertEquals(
            "fresh_non_resumed_live_session",
            setupBoundary.string("successConversation"),
        )
        assertEquals(
            "only_after_fresh_session_ready_and_success_announcement_finished",
            setupBoundary.string("microphoneResume"),
        )
        assertFalse(setupBoundary.boolean("internalTurnPersistence"))
        val dismissTarget = invariants.getValue("orbDismissTarget").jsonObject
        assertEquals(
            "same_current_overlay_host_as_orb_renderer",
            dismissTarget.string("windowOwner"),
        )
        assertEquals("same_as_orb_renderer", dismissTarget.string("windowType"))
        assertEquals("attached_after_orb_renderer", dismissTarget.string("zOrder"))
        assertEquals("not_visible", dismissTarget.string("attachmentFailure"))
        assertEquals("same_as_normal_runtime", dismissTarget.string("setupVisibility"))
        val setupFeedback = invariants.getValue("setupFeedback").jsonObject
        assertEquals(
            "localized_short_state_only_max_32_characters",
            setupFeedback.string("toast"),
        )
        assertEquals("localized_compact_status", setupFeedback.string("orbCaption"))
        assertEquals(
            "localized_full_instruction",
            setupFeedback.string("ongoingNotification"),
        )
        assertEquals(
            listOf("default", "ko", "vi"),
            setupFeedback.getValue("explicitUiLocales").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals(
            "android_default_resource_fallback",
            setupFeedback.string("otherUiLocales"),
        )
        assertEquals("in_app_language_setting", setupFeedback.string("uiLocaleSource"))
        val accessibility = invariants.getValue("accessibilityReadiness").jsonObject
        assertEquals(
            listOf("configured_service", "live_service_binding"),
            accessibility.getValue("requiredEvidence").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertFalse(accessibility.boolean("configuredSettingAloneIsReady"))
        assertEquals(
            "bounded_reconnect_wait_then_fresh_state_resolution",
            accessibility.string("configuredButUnbound"),
        )
        assertEquals("freshly_disabled_only", accessibility.string("settingsLaunch"))
        assertEquals(
            "stop_without_settings_then_retry_from_fresh_evidence",
            accessibility.string("stillConfiguredAfterWait"),
        )
        assertFalse(accessibility.boolean("serviceBoundAfterSettingRemovalIsReady"))
        val semantics = invariants.getValue("accessibilitySemantics").jsonObject
        assertEquals(
            "bounded_safe_descendant_labels_on_exact_action_owner",
            semantics.string("splitLabelAndAction"),
        )
        assertFalse(semantics.boolean("languageAndDeviceBranching"))
        assertEquals("never_inherit", semantics.string("editableOrProtectedDescendant"))
        assertEquals(
            "revalidate_exact_published_action_owner",
            semantics.string("dispatch"),
        )
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
            "seal_visual_evidence_then_apply_provider_capture_policy",
            protectedCheckpoint.string("capture"),
        )
        val capturePolicy = protectedCheckpoint.getValue("capturePolicy").jsonObject
        assertEquals("retain_projection", capturePolicy.string("sgt_adb_bridge"))
        assertEquals("release_projection", capturePolicy.string("shizuku_shell"))
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
        val guidanceLifecycle = protectedCheckpoint.getValue("guidanceLifecycle").jsonObject
        assertEquals(
            "replace_with_neutral_progress_before_adapter_wait",
            guidanceLifecycle.string("preCheckpoint"),
        )
        assertEquals(
            "reject_while_checkpoint_active",
            guidanceLifecycle.string("staleExternalProgress"),
        )
        assertEquals(
            "fresh_projection_required_only_after_release",
            guidanceLifecycle.string("awaitingProjection"),
        )
        assertEquals(
            "clear_after_fresh_post_attach_probe",
            guidanceLifecycle.string("providerReady"),
        )
        val projectionPrompt = protectedCheckpoint.getValue("projectionPrompt").jsonObject
        assertEquals("automatic_coordinator_reentry", projectionPrompt.string("launch"))
        assertEquals(
            "clear_coordinator_owned_external_surfaces_then_deliver_resume",
            projectionPrompt.string("existingCoordinatorTask"),
        )
        assertEquals(
            "create_resume_coordinator",
            projectionPrompt.string("missingCoordinatorTask"),
        )
        assertEquals(
            "explicit_immutable_internal_pending_intent_with_platform_bal_opt_in",
            projectionPrompt.string("backgroundLaunch"),
        )
        assertEquals(
            "retire_when_coordinator_reentry_pending",
            projectionPrompt.string("staleExternalResult"),
        )
        assertEquals(
            "exact_coordinator_token_ack_then_projection_launcher_dispatch",
            projectionPrompt.string("launchDispatch"),
        )
        assertEquals(
            "android_activity_result_then_fresh_projection_attach",
            projectionPrompt.string("completionReceipt"),
        )
        assertEquals("ongoing_notification_tap", projectionPrompt.string("fallback"))
        assertEquals(
            "discard_transient_checkpoint_then_normal_activation",
            projectionPrompt.string("processRestart"),
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
        assertEquals(
            "unseal_retained_projection_or_request_fresh_projection",
            cancelOutcome.string("nextStep"),
        )
        assertEquals(
            "unseal_retained_projection_or_attach_fresh_projection_to_existing_runtime",
            protectedCheckpoint.string("resume"),
        )
        assertFalse(protectedCheckpoint.boolean("providerSetupWhileSealed"))
        assertEquals(
            "blocked_until_retained_projection_unsealed_or_fresh_projection_attached",
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
            "resume_selected_setup_after_visual_evidence_restored",
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
            "os_owned_user_step",
            activePlatformSession.string("resolvedHandlerFullScreenSurface"),
        )
        assertEquals(
            "routine_navigation",
            activePlatformSession.string("sameApplicationOutsideExactSession"),
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
        assertEquals("purple_fill", powerPresentation.string("selectedMarker"))
        assertEquals(
            "star_icon_without_selected_fill",
            powerPresentation.string("recommendedMarker"),
        )
        assertEquals(
            "same_as_ordinary_unselected_choice",
            powerPresentation.string("recommendedBorder"),
        )
        val forgetPairing = powerPresentation.getValue("forgetPairing").jsonObject
        assertEquals(
            "compact_secondary_action_when_paired",
            forgetPairing.string("visibility"),
        )
        assertEquals(
            "delete_client_key_and_pairing_state",
            forgetPairing.string("credentialOutcome"),
        )
        assertEquals("standard", forgetPairing.string("selectionOutcome"))
        assertEquals("persisted_selection", forgetPairing.string("promptRefresh"))
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
        val readyFeedback = powerSelection.getValue("verifiedReadyFeedback").jsonObject
        assertEquals(
            "fresh_provider_probe_ready_after_visual_evidence_restored",
            readyFeedback.string("trigger"),
        )
        assertEquals(
            "immediate_short_localized_orb_caption_and_toast",
            readyFeedback.string("visual"),
        )
        assertEquals(
            "one_localized_app_owned_voice_message",
            readyFeedback.string("voice"),
        )
        assertEquals(
            "once_per_verified_ready_transition",
            readyFeedback.string("deduplication"),
        )
        assertEquals(
            "clear_pending_setup_without_reopening_or_extra_model_turn",
            readyFeedback.string("completion"),
        )
        val protectedChange = powerSelection
            .getValue("changeDuringProtectedCheckpoint")
            .jsonObject
        assertEquals("cancel", protectedChange.string("oldAdapter"))
        assertEquals(
            "remain_live_and_visually_sealed",
            protectedChange.string("runtime"),
        )
        assertEquals(
            "unseal_retained_projection_or_request_fresh_projection",
            protectedChange.string("nextStep"),
        )
        assertEquals(
            "start_only_after_visual_evidence_restored",
            protectedChange.string("selectedSetup"),
        )
        assertFalse(protectedChange.boolean("automationWhileToolsBlocked"))
        PhoneControlPowerChoice.entries.forEach { choice ->
            assertEquals(
                PhoneControlPowerSelectionRoute.RESUME_CAPTURE,
                phoneControlPowerSelectionRoute(choice, freshProjectionRequired = true),
            )
        }
        assertEquals(
            PhoneControlPowerSelectionRoute.NONE,
            phoneControlPowerSelectionRoute(
                PhoneControlPowerChoice.STANDARD,
                freshProjectionRequired = false,
            ),
        )
        PhoneControlPowerChoice.entries
            .filter { it.elevatedProviderId != null }
            .forEach { choice ->
                assertEquals(
                    PhoneControlPowerSelectionRoute.SETUP,
                    phoneControlPowerSelectionRoute(
                        choice,
                        freshProjectionRequired = false,
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
        assertElevatedSetupContracts(invariants)

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
