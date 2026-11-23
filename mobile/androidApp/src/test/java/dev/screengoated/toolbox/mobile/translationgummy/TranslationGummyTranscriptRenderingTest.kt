package dev.screengoated.toolbox.mobile.translationgummy

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Test

class TranslationGummyTranscriptRenderingTest {
    @Test
    fun `current transcript snapshots determine every visible pair`() {
        val fixturePath = "parity-fixtures/translation-gummy/transcript-rendering.json"
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) {
            it.parentFile
        }.first { File(it, fixturePath).isFile }
        val document = Json.parseToJsonElement(File(root, fixturePath).readText()).jsonObject
        assertEquals(1L, document.getValue("version").jsonPrimitive.long)
        for (case in document.getValue("cases").jsonArray) {
            val entry = case.jsonObject
            val name = entry.getValue("name").jsonPrimitive.content
            for (snapshot in entry.getValue("snapshots").jsonArray) {
                val state = snapshot.jsonObject
                val rows = state.getValue("items").jsonArray.map { value ->
                    val row = value.jsonObject
                    TranslationGummyTranscriptItem(
                        id = row.getValue("id").jsonPrimitive.long,
                        role = TranslationGummyTranscriptRole.valueOf(row.getValue("role").jsonPrimitive.content.uppercase()),
                        text = row.getValue("text").jsonPrimitive.content,
                        isFinal = row.getValue("isFinal").jsonPrimitive.boolean,
                        lang = row.getValue("lang").jsonPrimitive.content,
                        updatedAtMs = 0,
                    )
                }
                val actual = groupEntries(rows).filterIsInstance<TranscriptEntry.Pair>().map {
                    listOf(it.id.toString(), it.input, it.output)
                }
                val expected = state.getValue("expectedPairs").jsonArray.map { value ->
                    val pair = value.jsonObject
                    listOf("id", "input", "output").map { pair.getValue(it).jsonPrimitive.content }
                }
                assertEquals(name, expected, actual)
            }
        }
    }
}
