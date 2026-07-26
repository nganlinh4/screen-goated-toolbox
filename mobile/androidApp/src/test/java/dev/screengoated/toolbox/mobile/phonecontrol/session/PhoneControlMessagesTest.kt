package dev.screengoated.toolbox.mobile.phonecontrol.session

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

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
        assertTrue(instruction.contains("Try the requested capability"))
        assertTrue(instruction.contains("direct execution power"))
        assertTrue(instruction.contains("a snapshot-local node @id is not a surface target"))
        assertTrue(instruction.contains("ROUTING: highest-fidelity evidence."))
        assertTrue(instruction.contains("Interpret communicative intent, not grammatical form."))
        assertFalse(instruction.contains("element exactly as its observed @id"))
    }

    @Test
    fun `canonical contract acts before asking for resolvable device details`() {
        val prompt = Files.readAllBytes(canonicalPromptPath()).decodeToString()

        assertTrue(prompt.contains("Act first when action is requested."))
        assertTrue(prompt.contains("Recover silently."))
        assertTrue(prompt.contains("state only the actionable step."))
        assertTrue(prompt.contains("Resolve operational details from current evidence and tools"))
        assertTrue(prompt.contains("Routine requested actions proceed."))
        assertFalse(prompt.contains("protect your privacy", ignoreCase = true))
    }

    @Test
    fun `live endpoint and thinking match shared control fixture`() {
        val setup = Json.parseToJsonElement(
            buildPhoneControlSetupPayload(
                assets = PhoneControlContractAssets(
                    functionDeclarations = JsonArray(emptyList()),
                    canonicalPrompt = "Control ${GeneratedPhoneControlContract.PLATFORM_DEVICE_TOKEN}.",
                ),
                capabilityContext = "capabilities",
                voiceName = "Aoede",
            ),
        ).jsonObject.getValue("setup").jsonObject
        val fixture = Json.parseToJsonElement(
            Files.readAllBytes(fixturePath()).decodeToString(),
        ).jsonObject.getValue("live_session").jsonObject
        assertEquals(
            "models/${fixture.getValue("api_model").jsonPrimitive.content}",
            setup.getValue("model").jsonPrimitive.content,
        )
        assertEquals(
            fixture.getValue("thinking_config"),
            setup.getValue("generationConfig").jsonObject.getValue("thinkingConfig"),
        )
        assertEquals(
            fixture.getValue("realtime_input_config"),
            setup.getValue("realtimeInputConfig"),
        )
    }

    @Test
    fun `live speech remains multilingual without an app locale override`() {
        val setup = Json.parseToJsonElement(
            buildPhoneControlSetupPayload(
                assets = PhoneControlContractAssets(
                    functionDeclarations = JsonArray(emptyList()),
                    canonicalPrompt = "Control ${GeneratedPhoneControlContract.PLATFORM_DEVICE_TOKEN}.",
                ),
                capabilityContext = "capabilities",
                voiceName = "Aoede",
            ),
        ).jsonObject.getValue("setup").jsonObject

        assertTrue(setup.containsKey("inputAudioTranscription"))
        assertTrue(setup.containsKey("outputAudioTranscription"))
        assertFalse(setup.hasKeyRecursively("languageCode"))
    }

    private fun JsonElement.hasKeyRecursively(key: String): Boolean = when (this) {
        is JsonObject -> containsKey(key) || values.any { it.hasKeyRecursively(key) }
        is JsonArray -> any { it.hasKeyRecursively(key) }
        else -> false
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "phone-control", "model-chain.json"),
            Paths.get("..", "..", "parity-fixtures", "phone-control", "model-chain.json"),
            Paths.get("parity-fixtures", "phone-control", "model-chain.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Missing Phone Control model-chain fixture. Tried: $candidates")
    }

    private fun canonicalPromptPath(): Path {
        val candidates = listOf(
            Paths.get("..", "..", "src", "overlay", "computer_control", "uia_task", "prompt_core.txt"),
            Paths.get("..", "src", "overlay", "computer_control", "uia_task", "prompt_core.txt"),
            Paths.get("src", "overlay", "computer_control", "uia_task", "prompt_core.txt"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Missing canonical Computer Control prompt. Tried: $candidates")
    }
}
