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
        assertEquals(1L, fixture.getValue("schemaVersion").jsonPrimitive.long)
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
        assertFalse(invariants.boolean("userFacingSelfTest"))
        assertTrue(invariants.boolean("oneExternalUserStepAtATime"))
        assertTrue(invariants.boolean("reprobeAfterReturn"))
        val navigation = invariants.getValue("settingsNavigation").jsonObject
        assertEquals(
            "runtime_app_label_on_resolved_settings_package",
            navigation.string("targetIdentity"),
        )
        assertTrue(navigation.boolean("mayScroll"))
        assertTrue(navigation.boolean("mayOpenAppRow"))
        assertFalse(navigation.boolean("mayTogglePermission"))
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
        val shizuku = invariants.getValue("shizukuSetup").jsonObject
        assertTrue(shizuku.boolean("feedbackBeforeExternalStep"))
        assertEquals("structural_probe_state", shizuku.string("plannerInput"))
        assertEquals("complete", shizuku.string("ready"))
        assertEquals("request_permission", shizuku.string("binderReadyPermissionMissing"))
        assertEquals("open_manager", shizuku.string("installedServiceStoppedOrGrantRevoked"))
        assertEquals(
            "open_store_with_official_download_fallback",
            shizuku.string("packageMissingOrApiUnsupported"),
        )
        assertEquals("on_external_return_or_binder_event", shizuku.string("reprobe"))
        assertEquals("stop_without_reopening", shizuku.string("unchangedExternalState"))
        assertEquals("user_step", shizuku.string("androidOwnedPairingAndTrust"))
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

        fixture.getValue("cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val snapshot = case.getValue("snapshot").jsonObject
            val actual = nextPhoneControlActivationStep(
                PhoneControlActivationSnapshot(
                    apiKeyReady = snapshot.boolean("apiKey"),
                    microphoneReady = snapshot.boolean("microphone"),
                    notificationsReady = snapshot.boolean("notifications"),
                    notificationPrompted = snapshot.boolean("notificationPrompted"),
                    accessibilityEnabled = snapshot.boolean("accessibilityEnabled"),
                    overlayReady = snapshot.boolean("overlay"),
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
