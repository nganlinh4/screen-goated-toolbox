package dev.screengoated.toolbox.mobile.preset

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject

internal fun buildGeminiDebugPayload(
    fullName: String,
    prompt: String,
    inputText: String,
    streamingEnabled: Boolean,
): String {
    val payload = buildJsonObject {
        putJsonArray("contents") {
            add(
                buildJsonObject {
                    put("role", "user")
                    putJsonArray("parts") {
                        add(buildJsonObject { put("text", "$prompt\n\n$inputText") })
                    }
                },
            )
        }
        PresetModelCatalog.geminiThinkingConfig(fullName)?.let { thinking ->
            putJsonObject("generationConfig") {
                putJsonObject("thinkingConfig") {
                    thinking.forEach { (key, value) ->
                        when (value) {
                            is Boolean -> put(key, value)
                            is Number -> put(key, value.toDouble())
                            else -> put(key, value.toString())
                        }
                    }
                }
            }
        }
        if (PresetModelCatalog.supportsSearchByName(fullName)) {
            putJsonArray("tools") {
                add(buildJsonObject { putJsonObject("url_context") {} })
                add(buildJsonObject { putJsonObject("google_search") {} })
            }
        }
        put("stream", streamingEnabled)
    }
    return debugJson.encodeToString(JsonObject.serializer(), payload)
}

internal fun buildGroqCompoundDebugPayload(
    fullName: String,
    prompt: String,
    inputText: String,
): String {
    val payload = buildJsonObject {
        put("model", fullName)
        putJsonArray("messages") {
            add(
                buildJsonObject {
                    put("role", "system")
                    put(
                        "content",
                        "IMPORTANT: Limit yourself to a maximum of 3 tool calls total. Make 1-2 focused searches, then answer. Do not visit websites unless absolutely necessary. Be efficient.",
                    )
                },
            )
            add(
                buildJsonObject {
                    put("role", "user")
                    put("content", "$prompt\n\n$inputText")
                },
            )
        }
        put("temperature", 1)
        put("max_tokens", 8192)
        put("stream", false)
        putJsonObject("compound_custom") {
            putJsonObject("tools") {
                putJsonArray("enabled_tools") {
                    add(JsonPrimitive("web_search"))
                    add(JsonPrimitive("visit_website"))
                }
            }
        }
    }
    return debugJson.encodeToString(JsonObject.serializer(), payload)
}

internal fun buildOpenAiCompatibleDebugPayload(
    fullName: String,
    prompt: String,
    inputText: String,
): String {
    val payload = buildJsonObject {
        put("model", fullName)
        putJsonArray("messages") {
            add(
                buildJsonObject {
                    put("role", "user")
                    put("content", "$prompt\n\n$inputText")
                },
            )
        }
        put("stream", true)
    }
    return debugJson.encodeToString(JsonObject.serializer(), payload)
}
