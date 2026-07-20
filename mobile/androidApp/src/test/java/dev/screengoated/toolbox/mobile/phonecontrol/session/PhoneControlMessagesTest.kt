package dev.screengoated.toolbox.mobile.phonecontrol.session

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlMessagesTest {
    @Test
    fun `android text tools require a surface token rather than a node id`() {
        val payload = buildPhoneControlSetupPayload(
            assets = PhoneControlContractAssets(
                functionDeclarations = JsonArray(emptyList()),
                canonicalPrompt = "Control ${GeneratedPhoneControlContract.PLATFORM_DEVICE_TOKEN}.",
            ),
            capabilityContext = "capabilities",
            voiceName = "Aoede",
        )

        val instruction = Json.parseToJsonElement(payload)
            .jsonObject
            .getValue("setup")
            .jsonObject
            .getValue("systemInstruction")
            .jsonObject
            .getValue("parts")
            .jsonArray[0]
            .jsonObject
            .getValue("text")
            .jsonPrimitive
            .content

        assertTrue(instruction.contains("surface token returned by list_windows"))
        assertTrue(instruction.contains("a snapshot-local node @id is not a surface target"))
        assertFalse(instruction.contains("element exactly as its observed @id"))
    }
}
