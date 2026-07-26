package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.double
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Test

class PhoneControlOrbContractTest {
    @Test
    fun dragDismissUsesTheSharedSingleTargetContract() {
        val fixture = Json.parseToJsonElement(fixtureFile().readText()).jsonObject
        val invariants = fixture.getValue("invariants").jsonObject

        assertEquals("shared_android_dismiss_bubble", invariants.string("dragDismissOwner"))
        assertEquals("single_bottom_target", invariants.string("dragDismissTargets"))
        assertEquals("stop_phone_control_service", invariants.string("dragDismissCommit"))
        assertEquals("hide_target_and_keep_session", invariants.string("dragDismissCancel"))
        assertEquals("current_raw_pointer", invariants.string("dragDismissCoordinates"))
    }

    @Test
    fun audioReactionMatchesTheCanonicalWindowsPcmMapping() {
        val fixture = Json.parseToJsonElement(fixtureFile().readText()).jsonObject
        val audio = fixture.getValue("invariants").jsonObject
            .getValue("audioReaction").jsonObject

        assertEquals(120.0 / 32768.0, audio.getValue("voicedThreshold").jsonPrimitive.double, 0.0)
        assertEquals(32768.0 / 4000.0, audio.getValue("gain").jsonPrimitive.double, 0.0)
        assertEquals(80L, audio.getValue("updateIntervalMs").jsonPrimitive.long)
    }

    private fun fixtureFile(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, FIXTURE_PATH) }
            .firstOrNull(File::isFile)
            ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private fun kotlinx.serialization.json.JsonObject.string(field: String): String =
        getValue(field).jsonPrimitive.content

    private companion object {
        const val FIXTURE_PATH = "parity-fixtures/phone-control/orb-contract.json"
    }
}
