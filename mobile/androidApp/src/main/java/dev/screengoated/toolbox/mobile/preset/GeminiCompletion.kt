package dev.screengoated.toolbox.mobile.preset

import org.json.JSONObject
import java.io.IOException

private fun invalidGemini(message: String): Nothing =
    throw IOException("PROVIDER_RESPONSE_INVALID:$message")

private class GeminiCompletion {
    private val text = StringBuilder()
    private var stopped = false

    fun push(root: JSONObject): GeminiDelta {
        if (!root.isNull("error")) invalidGemini("Provider response error: ${root.get("error")}")
        val block = root.optJSONObject("promptFeedback")?.optString("blockReason").orEmpty()
        if (block.isNotEmpty() && block != "BLOCK_REASON_UNSPECIFIED") {
            invalidGemini("Provider blocked response: $block")
        }
        val candidates = root.optJSONArray("candidates")
        val candidate = candidates?.let { rows ->
            (0 until rows.length()).map { index ->
                rows.optJSONObject(index) ?: invalidGemini("Malformed provider candidate")
            }.firstOrNull { !it.has("index") || it.optInt("index", -1) == 0 }
        }
        if (candidate == null) {
            if (!root.has("usageMetadata")) invalidGemini("Provider returned no candidates")
            return GeminiDelta()
        }
        val reason = candidate.opt("finishReason").takeUnless { it == JSONObject.NULL }
        val stop = when (reason) {
            null -> false
            "STOP" -> true
            "MAX_TOKENS" -> invalidGemini("Provider completion token limit reached")
            else -> invalidGemini("Provider completion did not succeed: $reason")
        }
        val delta = StringBuilder()
        var thought = false
        if (candidate.has("content")) {
            val parts = candidate.optJSONObject("content")?.optJSONArray("parts")
                ?: invalidGemini("Malformed provider content")
            for (index in 0 until parts.length()) {
                val part = parts.optJSONObject(index) ?: invalidGemini("Malformed provider part")
                if (part.has("text")) {
                    val value = part.opt("text") as? String ?: invalidGemini("Provider content is not text")
                    if (part.optBoolean("thought", false)) thought = thought || value.isNotEmpty()
                    else delta.append(value)
                } else if (!part.has("thoughtSignature")) {
                    invalidGemini("Provider returned non-text content")
                }
            }
        }
        if (stopped && (delta.isNotEmpty() || thought)) invalidGemini("Provider emitted output after completion")
        stopped = stopped || stop
        text.append(delta)
        return GeminiDelta(content = delta.toString(), reasoning = thought)
    }

    fun finish(): String {
        if (!stopped) invalidGemini("Provider stream ended before completion")
        if (text.isBlank()) invalidGemini("Provider returned no output content")
        return text.toString()
    }
}

internal fun parseGeminiCompletion(root: JSONObject): String = GeminiCompletion().run {
    push(root)
    finish()
}

internal fun consumeGeminiStream(lines: Sequence<String>, onDelta: (GeminiDelta) -> Unit): String {
    val completion = GeminiCompletion()
    val event = mutableListOf<String>()
    fun dispatch() {
        if (event.isEmpty()) return
        val data = event.joinToString("\n")
        event.clear()
        if (data.trim() == "[DONE]") return
        val root = try { JSONObject(data) } catch (_: org.json.JSONException) {
            invalidGemini("Malformed provider stream event")
        }
        val delta = completion.push(root)
        if (delta.content.isNotEmpty() || delta.reasoning) onDelta(delta)
    }
    lines.forEach { line ->
        if (line.isEmpty()) dispatch()
        else if (line.startsWith("data:")) event.add(line.removePrefix("data:").removePrefix(" "))
    }
    dispatch()
    return completion.finish()
}
