package dev.screengoated.toolbox.mobile.preset

import java.io.File
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class SpeechLanguageParityTest {
    private fun repoFile(path: String): File = generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
        .map { File(it, path) }.first(File::isFile)

    @Test fun sharedCatalogAndNormalization() {
        val fixture = JSONObject(repoFile("parity-fixtures/preset-system/speech-language.json").readText())
        val catalog = JSONArray(repoFile(fixture.getString("catalog")).readText())
        assertEquals((0 until catalog.length()).map {
            catalog.getJSONObject(it).let { row -> row.getString("value") to row.getString("label") }
        }, whisperSpeechLanguages)
        val cases = fixture.getJSONArray("cases")
        for (index in 0 until cases.length()) {
            val row = cases.getJSONObject(index)
            assertEquals(if (row.isNull("expected")) null else row.getString("expected"),
                whisperLanguageCode(if (row.isNull("input")) null else row.getString("input")))
        }
    }
}
