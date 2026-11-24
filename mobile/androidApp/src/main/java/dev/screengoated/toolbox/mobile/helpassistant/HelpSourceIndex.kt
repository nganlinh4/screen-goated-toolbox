package dev.screengoated.toolbox.mobile.helpassistant

import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayInputStream
import java.io.IOException
import java.util.Base64
import kotlin.math.sqrt

internal const val HELP_SOURCE_DIMENSIONS = 768
internal const val HELP_SOURCE_TOP_K = 6
internal const val HELP_SOURCE_QUERY_PREFIX = "task: question answering | query: "

internal fun parseHelpSourceIndex(
    bytes: ByteArray,
    platform: String,
    expectedEntries: Int?,
): List<ChunkEntry> = ByteArrayInputStream(bytes).bufferedReader(Charsets.UTF_8).use { input ->
    val headerLine = requireNotNull(input.readLine())
    require(headerLine.length <= 1024)
    val header = JSONObject(headerLine)
    require(header.getInt("schemaVersion") == 2)
    require(header.getInt("dimensions") == HELP_SOURCE_DIMENSIONS)
    require(header.getString("embeddingModel") == GeneratedLiveModelCatalog.GEMINI_EMBEDDING_API_MODEL)
    require(header.getString("sourceRevision").matches(Regex("[a-f0-9]{40}")))
    val entries = mutableListOf<ChunkEntry>()
    var count = 0
    input.lineSequence().forEach { line ->
        count++
        require(count <= SOURCE_MAXIMUM_ENTRIES && line.length <= 32 * 1024)
        val item = JSONObject(line)
        val path = item.getString("path")
        val text = item.getString("text")
        val startLine = item.getInt("startLine")
        val platforms = item.getJSONArray("platforms")
        val supported = (0 until platforms.length()).map(platforms::getString)
        require(path.isNotEmpty() && path.length <= 256)
        require(text.isNotEmpty() && text.length <= 128 * 1024 && startLine > 0)
        require(supported.isNotEmpty() && supported.all { it == "windows" || it == "android" })
        val vector = Base64.getDecoder().decode(item.getString("vector"))
        require(vector.size == HELP_SOURCE_DIMENSIONS && vector.any { it != 0.toByte() })
        if (platform in supported) entries += ChunkEntry(path, text, startLine, vector)
    }
    require(count > 0 && (expectedEntries == null || count == expectedEntries))
    entries
}

internal fun retrieveHelpChunks(
    client: OkHttpClient,
    index: List<ChunkEntry>,
    question: String,
    key: String,
): List<ChunkEntry> {
    if (index.firstOrNull()?.vector == null) return rankHelpAssistantChunks(index, question)
    val query = runCatching { embedQuestion(client, question, key) }.getOrNull()
        ?: return rankHelpAssistantChunks(index, question).take(HELP_SOURCE_TOP_K)
    return rankHelpSourceChunks(index, query)
}

internal fun rankHelpSourceChunks(index: List<ChunkEntry>, query: DoubleArray): List<ChunkEntry> =
    index.map { chunk ->
        val vector = requireNotNull(chunk.vector)
        var dot = 0.0
        var norm = 0.0
        for (coordinate in vector.indices) {
            val value = vector[coordinate].toDouble()
            dot += value * query[coordinate]
            norm += value * value
        }
        chunk to dot / sqrt(norm)
    }.sortedByDescending { (_, score) -> score }
        .take(HELP_SOURCE_TOP_K)
        .map { (chunk, _) -> chunk }

private fun embedQuestion(client: OkHttpClient, question: String, key: String): DoubleArray {
    val model = GeneratedLiveModelCatalog.GEMINI_EMBEDDING_API_MODEL
    val payload = JSONObject()
        .put("content", JSONObject().put(
            "parts", JSONArray().put(JSONObject().put("text", HELP_SOURCE_QUERY_PREFIX + question)),
        ))
        .put("outputDimensionality", HELP_SOURCE_DIMENSIONS)
    val request = Request.Builder()
        .url("https://generativelanguage.googleapis.com/v1beta/models/$model:embedContent")
        .header("x-goog-api-key", key)
        .post(payload.toString().toRequestBody("application/json; charset=utf-8".toMediaType()))
        .build()
    return client.newCall(request).execute().use { response ->
        if (!response.isSuccessful) throw IOException("Embedding request failed: HTTP ${response.code}")
        val root = JSONObject(response.body.string())
        val values = (root.optJSONArray("embeddings")?.optJSONObject(0)
            ?: root.getJSONObject("embedding")).getJSONArray("values")
        require(values.length() == HELP_SOURCE_DIMENSIONS)
        DoubleArray(HELP_SOURCE_DIMENSIONS, values::getDouble).also { vector ->
            require(vector.all(Double::isFinite) && vector.any { it != 0.0 })
        }
    }
}
