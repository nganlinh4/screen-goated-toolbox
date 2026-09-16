package dev.screengoated.toolbox.mobile.preset

import org.json.JSONObject
import java.io.BufferedReader
import java.io.IOException

private fun completionRoot(payload: String): JSONObject = try {
    JSONObject(payload)
} catch (error: org.json.JSONException) {
    throw IOException("PROVIDER_RESPONSE_INVALID:Malformed provider stream event", error)
}

private fun checkCompletionError(root: JSONObject) {
    if (!root.isNull("error")) {
        val error = root.opt("error")
        val code = (error as? JSONObject)?.opt("code")?.let { " $it" }.orEmpty()
        val message = (error as? JSONObject)?.optString("message") ?: error.toString()
        throw IOException("PROVIDER_RESPONSE_INVALID:Provider response error$code: ${message.take(512)}")
    }
}

private fun checkCompletionFinish(choice: JSONObject): Boolean {
    if (choice.isNull("finish_reason")) return false
    return when (val reason = choice.opt("finish_reason")) {
        "stop" -> true
        "length" -> throw IOException("PROVIDER_RESPONSE_INVALID:Provider completion token limit reached")
        else -> throw IOException("PROVIDER_RESPONSE_INVALID:Provider completion did not succeed: $reason")
    }
}

private fun firstCompletionChoice(root: JSONObject): JSONObject? {
    val choices = root.optJSONArray("choices") ?: return null
    for (index in 0 until choices.length()) {
        val choice = choices.optJSONObject(index) ?: continue
        if (!choice.has("index") || choice.opt("index") == 0) return choice
    }
    return null
}

internal fun parseOpenAiCompletion(payload: String): String {
    val root = completionRoot(payload)
    checkCompletionError(root)
    val choice = firstCompletionChoice(root) ?: throw IOException("PROVIDER_RESPONSE_INVALID:Provider returned no output content")
    checkCompletionFinish(choice)
    val content = choice.optJSONObject("message")?.opt("content") as? String
    if (content.isNullOrBlank()) throw IOException("PROVIDER_RESPONSE_INVALID:Provider returned no output content")
    return content
}

private fun completionDelta(choice: JSONObject): OpenAiDelta {
    val delta = choice.optJSONObject("delta") ?: throw IOException("PROVIDER_RESPONSE_INVALID:Malformed provider stream event: missing delta")
    val content = if (delta.isNull("content")) "" else delta.opt("content") as? String
        ?: throw IOException("PROVIDER_RESPONSE_INVALID:Malformed provider stream event: content is not text")
    val reasoning = (delta.opt("reasoning") ?: delta.opt("reasoning_content")) as? String ?: ""
    return OpenAiDelta(content, reasoning)
}

internal fun extractOpenAiDelta(payload: String): OpenAiDelta {
    val root = completionRoot(payload)
    checkCompletionError(root)
    val choice = firstCompletionChoice(root) ?: return OpenAiDelta()
    checkCompletionFinish(choice)
    return completionDelta(choice)
}

internal fun consumeOpenAiCompletion(
    reader: BufferedReader,
    checkCancellation: () -> Unit,
    onEvent: (OpenAiDelta) -> Unit,
): String {
    val content = StringBuilder()
    val event = mutableListOf<String>()
    var complete = false
    var done = false
    fun dispatch(): Boolean {
        if (event.isEmpty()) return false
        val data = event.joinToString("\n")
        event.clear()
        if (data.trim() == "[DONE]") {
            done = true
            return true
        }
        val root = completionRoot(data)
        checkCompletionError(root)
        val choices = root.optJSONArray("choices") ?: throw IOException("PROVIDER_RESPONSE_INVALID:Malformed provider stream event: missing choices")
        if (choices.length() == 0) return false
        firstCompletionChoice(root)?.let { choice ->
            val finished = checkCompletionFinish(choice)
            val delta = completionDelta(choice)
            if (complete) {
                val fields = choice.getJSONObject("delta")
                val empty = fields.keys().asSequence().all { key ->
                    val value = fields.opt(key)
                    key == "role" || fields.isNull(key) || value == "" ||
                        (value is org.json.JSONArray && value.length() == 0)
                }
                if (!empty) throw IOException("PROVIDER_RESPONSE_INVALID:Provider emitted output after completion")
                return false
            }
            complete = finished
            content.append(delta.content)
            onEvent(delta)
        }
        return false
    }
    while (true) {
        checkCancellation()
        val line = reader.readLine() ?: break
        if (line.isEmpty()) {
            if (dispatch()) break
        } else if (line.startsWith("data:")) {
            event.add(line.removePrefix("data:").removePrefix(" "))
        }
    }
    dispatch()
    if (!complete && !done) throw IOException("PROVIDER_RESPONSE_INVALID:Provider stream ended before completion")
    if (content.isBlank()) throw IOException("PROVIDER_RESPONSE_INVALID:Provider returned no output content")
    return content.toString()
}
