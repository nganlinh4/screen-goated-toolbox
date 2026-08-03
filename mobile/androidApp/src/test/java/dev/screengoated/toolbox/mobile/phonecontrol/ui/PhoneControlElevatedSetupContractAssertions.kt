package dev.screengoated.toolbox.mobile.phonecontrol.ui

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuBridgeCondition
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuBridgeProbe
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue

internal fun assertElevatedSetupContracts(invariants: JsonObject) {
    val shizuku = invariants.getValue("shizukuSetup").jsonObject
    assertTrue(shizuku.boolean("feedbackBeforeExternalStep"))
    assertEquals(
        listOf("orb_caption", "ongoing_notification"),
        shizuku.getValue("durableFeedbackSurfaces").jsonArray.map {
            it.jsonPrimitive.content
        },
    )
    val guidancePresentation = shizuku.getValue("guidancePresentation").jsonObject
    assertEquals("compact_non_obscuring_status", guidancePresentation.string("orbCaption"))
    assertEquals("full_persistent_instruction", guidancePresentation.string("ongoingNotification"))
    assertEquals("short_state_only", guidancePresentation.string("toast"))
    assertEquals("structural_probe_state", shizuku.string("plannerInput"))
    val liveAutomation = shizuku.getValue("liveAutomation").jsonObject
    assertEquals("user_power_authority_selection", liveAutomation.string("trigger"))
    assertEquals("one_bounded_goal_when_idle", liveAutomation.string("turnBoundary"))
    assertEquals("normal_full_catalog", liveAutomation.string("catalog"))
    assertEquals("silent_internal_turn", liveAutomation.string("presentation"))
    assertEquals("normal_semantic_and_vision_tools", liveAutomation.string("navigation"))
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
    assertEquals("on_package_change_external_return_or_binder_event", shizuku.string("reprobe"))
    assertEquals("open_manager_without_reselection", shizuku.string("packageInstalled"))
    assertEquals(
        "remain_selected_pending_without_reopening",
        shizuku.string("unchangedExternalState"),
    )
    assertEquals("user_step", shizuku.string("androidPlayInstallConfirmation"))
    assertEquals("user_step", shizuku.string("androidOwnedPairingAndTrust"))
    assertEquals("sealed_local_adapter_or_user_step", shizuku.string("oneTimePairingCodeTransfer"))
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
        setOf("play", "full"),
        firstPartyAdb.getValue("distribution").jsonArray.map { it.jsonPrimitive.content }.toSet(),
    )
    assertEquals("one_fresh_probe_without_automatic_reopen", firstPartyAdb.string("externalReturn"))
    assertEquals(
        "remain_selected_pending_until_fresh_evidence_or_explicit_retry",
        firstPartyAdb.string("unchangedExternalState"),
    )
    assertEquals(
        "structurally_verified_pairing_surface_before_visual_seal",
        firstPartyAdb.string("handoffReadiness"),
    )
    val navigation = firstPartyAdb.getValue("navigationOrchestration").jsonObject
    assertEquals("provider_owned_semantic_setup_contract", navigation.string("goalInput"))
    assertEquals("armed_while_silent_navigation_goal_is_active", navigation.string("checkpointMonitor"))
    assertEquals("local_structure_only_without_secret_export", navigation.string("checkpointDetection"))
    assertEquals(
        "checkpoint_or_fresh_authority_probe_required",
        navigation.string("modelCompletionPostcondition"),
    )
    assertEquals(
        "bounded_fresh_observation_continuation_within_original_deadline",
        navigation.string("intermediateCompletion"),
    )
    assertEquals(
        "retire_hidden_owner_clear_stale_guidance_leave_provider_selected_pending",
        navigation.string("exhaustedNavigation"),
    )
    assertEquals(
        "block_new_tools_settle_owned_action_then_retire_hidden_generation",
        navigation.string("modelBoundary"),
    )
    assertFalse(navigation.boolean("normalDoneSemanticsChanged"))
    assertTrue(navigation.boolean("publicSettingsEntryOnly"))
    assertFalse(navigation.boolean("privateSettingsComponentAllowed"))

    val bridgeProcess = firstPartyAdb.getValue("bridgeProcessIsolation").jsonObject
    assertEquals("primary_application_process_only", bridgeProcess.string("fullApplicationContainer"))
    assertEquals("service_owned_minimal_runtime", bridgeProcess.string("dedicatedProcessStartup"))
    assertEquals(
        "separate_durable_process_journal_merged_by_collector",
        bridgeProcess.string("diagnostics"),
    )
    assertEquals(
        listOf("service_lifecycle", "connect_terminal", "pair_terminal", "authority_terminal"),
        bridgeProcess.getValue("diagnosticPhases").jsonArray.map { it.jsonPrimitive.content },
    )
    assertFalse(bridgeProcess.boolean("unrelatedSubsystemInitialization"))
    assertFalse(bridgeProcess.boolean("timeoutMaskingAllowed"))
    assertTrue(bridgeProcess.boolean("distributionParity"))
    assertEquals(
        "per_application_uid_key_and_pairing_state_with_identical_flavor_behavior",
        firstPartyAdb.string("packageIsolation"),
    )
    assertEquals("single_end_to_end_monotonic_deadline", firstPartyAdb.string("pairingDeadline"))
    assertEquals(
        "sealed_model_evidence_with_projection_retained",
        firstPartyAdb.string("captureLifecycle"),
    )
    assertEquals(
        "serialize_then_forget_abandoned_client_key_after_terminal_return",
        firstPartyAdb.string("cancelDuringPairing"),
    )
    assertEquals(
        "persist_pairing_then_bounded_reconnect_without_code_reentry",
        firstPartyAdb.string("pairingEstablishedBeforeConnect"),
    )
    assertEquals(
        "unseal_existing_projection_then_fresh_probe",
        firstPartyAdb.string("postPairingProjectionResume"),
    )
    assertEquals(
        "paired_client_key_and_persisted_adb_mdns_identity_family",
        firstPartyAdb.string("reconnect"),
    )
}

private fun JsonObject.boolean(field: String): Boolean = getValue(field).jsonPrimitive.boolean

private fun JsonObject.string(field: String): String = getValue(field).jsonPrimitive.content
