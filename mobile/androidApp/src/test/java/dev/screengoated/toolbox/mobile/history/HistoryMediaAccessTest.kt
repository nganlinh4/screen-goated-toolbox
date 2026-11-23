package dev.screengoated.toolbox.mobile.history

import java.io.FileNotFoundException
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class HistoryMediaAccessTest {
    private val proof = HistoryMediaProof("1:2", 3, "a".repeat(64))
    private val item = HistoryItem(7, "time", HistoryType.TEXT, "text", "entry.txt", proof)
    private val parts = listOf("7", "entry.txt", "1:2", "3", "a".repeat(64), historyItemDigest(item))
    private val request = historyMediaRequest(parts)

    @Test fun sharedMediaNamesRequireOneSafeLeaf() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("history-ui/media-names.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        contract.getValue("cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("valid").jsonPrimitive.boolean,
                historyMediaLeaf(case.getValue("value").jsonPrimitive.content))
        }
    }

    @Test fun sharedMediaTypesMatchTheirFileExtension() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("history-ui/media-types.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        contract.getValue("cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val type = HistoryType.valueOf(case.getValue("type").jsonPrimitive.content)
            assertEquals(name, case.getValue("valid").jsonPrimitive.boolean, historyMediaNameMatches(type, name))
        }
    }

    @Test fun validatesCapabilityStructure() {
        historyMediaRequest(parts)
        for ((index, value) in listOf(0 to "x", 1 to "../entry", 2 to "1:x", 3 to "-1", 4 to "z".repeat(64), 5 to "bad")) {
            val invalid = parts.toMutableList().apply { this[index] = value }
            assertThrows(FileNotFoundException::class.java) { historyMediaRequest(invalid) }
        }
        assertThrows(FileNotFoundException::class.java) { historyMediaRequest(parts.dropLast(1)) }
    }

    @Test fun authorizesOnlyCurrentUnambiguousItemAndProof() {
        authorizeHistoryMedia(HistoryUiState(items = listOf(item)), request)
        val invalidStates = listOf(
            HistoryUiState(),
            HistoryUiState(items = listOf(item, item)),
            HistoryUiState(items = listOf(item), storageUnavailable = true),
            HistoryUiState(items = listOf(item.copy(mediaPath = "other.txt"))),
            HistoryUiState(items = listOf(item.copy(mediaProof = request.proof.copy(identity = "1:3")))),
            HistoryUiState(items = listOf(item.copy(text = "changed"))),
        )
        invalidStates.forEach { state -> assertThrows(FileNotFoundException::class.java) { authorizeHistoryMedia(state, request) } }
        val executable = item.copy(mediaPath = "entry.txt.exe")
        val forged = request.copy(name = executable.mediaPath, itemDigest = historyItemDigest(executable))
        assertThrows(FileNotFoundException::class.java) {
            authorizeHistoryMedia(HistoryUiState(items = listOf(executable)), forged)
        }
    }

    @Test fun permitsLegacyItemWithObservedProof() {
        val legacy = item.copy(mediaProof = null)
        authorizeHistoryMedia(HistoryUiState(items = listOf(legacy)), request.copy(itemDigest = historyItemDigest(legacy)))
    }
}
