package dev.screengoated.toolbox.mobile.service.parakeet

import org.json.JSONObject
import java.io.File

internal class SentencePieceTokenizer(tokenizerJsonFile: File) {

    private val idToToken: Map<Int, String>
    val vocabSize: Int
    val blankId: Int
    val eouId: Int

    init {
        val json = JSONObject(tokenizerJsonFile.readText())
        val model = json.getJSONObject("model")
        val vocabObj = model.getJSONObject("vocab")

        val map = mutableMapOf<Int, String>()
        val keys = vocabObj.keys()
        while (keys.hasNext()) {
            val token = keys.next()
            val id = vocabObj.getInt(token)
            map[id] = token
        }
        idToToken = map
        vocabSize = map.size

        val rawBlank = vocabSize - 1
        blankId = if (rawBlank < 1000) 1026 else rawBlank

        eouId = map.entries.firstOrNull { it.value == "<EOU>" }?.key ?: 1024
    }

    fun decode(ids: IntArray): String {
        val sb = StringBuilder()
        for (id in ids) {
            val token = idToToken[id] ?: continue
            sb.append(token)
        }
        return sb.toString()
    }
}
