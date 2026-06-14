package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.TranslationPatch
import dev.screengoated.toolbox.mobile.shared.live.TranslationRequest
import dev.screengoated.toolbox.mobile.shared.live.TranslationResponse
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException

// Prompt-building, JSON request shaping, and response parsing/validation
// extracted from RealtimeTranslationClient. These are pure helpers with no
// dependency on the HTTP client or session state.

internal fun RealtimeTranslationClient.cerebrasMessages(
    request: TranslationRequest,
    targetLanguage: String,
): JSONArray {
    val messages = JSONArray()
    messages.put(
        JSONObject()
            .put("role", "system")
            .put(
                "content",
                "You translate live transcript windows into JSON source patches. Respond with JSON only.",
            ),
    )
    request.history.forEach { entry ->
        messages.put(
            JSONObject()
                .put("role", "user")
                .put("content", "Translate to $targetLanguage:\n${entry.source}"),
        )
        messages.put(
            JSONObject()
                .put("role", "assistant")
                .put("content", entry.translation),
        )
    }
    messages.put(
        JSONObject()
            .put("role", "user")
            .put("content", buildStructuredPrompt(request, targetLanguage)),
    )
    return messages
}

internal fun RealtimeTranslationClient.buildStructuredPrompt(
    request: TranslationRequest,
    targetLanguage: String,
): String {
    val history = JSONArray().apply {
        request.history.forEach { entry ->
            put(
                JSONObject()
                    .put("source", entry.source)
                    .put("translation", entry.translation),
            )
        }
    }
    val expectedPatches = JSONArray().apply {
        if (request.finalizedSource.isNotBlank()) {
            put(
                JSONObject()
                    .put("sourceStart", request.sourceStart)
                    .put("sourceEnd", request.finalizedSourceEnd)
                    .put("state", "final"),
            )
        }
        if (request.draftSource.isNotBlank()) {
            put(
                JSONObject()
                    .put("sourceStart", request.draftSourceStart)
                    .put("sourceEnd", request.sourceEnd)
                    .put("state", "draft"),
            )
        }
    }
    val window = JSONObject()
        .put("sourceStart", request.sourceStart)
        .put("sourceEnd", request.sourceEnd)
        .put("pendingSource", request.pendingSource)
        .put("finalizedSource", request.finalizedSource)
        .put("draftSource", request.draftSource)
        .put("previousDraftTranslation", request.previousDraftTranslation)

    return buildString {
        append("You are a professional live translator.\n")
        append("Translate only the provided source window into ")
        append(targetLanguage)
        append(".\n")
        append("Return JSON with a single key named patches.\n")
        append("Each patch must keep the exact sourceStart/sourceEnd values from expectedPatches.\n")
        append("Use state=\"final\" for the finalized source span and state=\"draft\" for the trailing unfinished span.\n")
        append("Do not add commentary, markdown, or extra keys.\n\n")
        append("Recent committed context:\n")
        append(history.toString())
        append("\n\n")
        append("Current source window:\n")
        append(window.toString())
        append("\n\n")
        append("Expected patches:\n")
        append(expectedPatches.toString())
    }
}

internal fun RealtimeTranslationClient.cerebrasResponseFormat(): JSONObject {
    val patchSchema = JSONObject()
        .put("type", "object")
        .put(
            "properties",
            JSONObject()
                .put("sourceStart", JSONObject().put("type", "integer"))
                .put("sourceEnd", JSONObject().put("type", "integer"))
                .put(
                    "state",
                    JSONObject()
                        .put("type", "string")
                        .put("enum", JSONArray().put("final").put("draft")),
                )
                .put("translation", JSONObject().put("type", "string")),
        )
        .put(
            "required",
            JSONArray()
                .put("sourceStart")
                .put("sourceEnd")
                .put("state")
                .put("translation"),
        )
        .put("additionalProperties", false)

    val schema = JSONObject()
        .put("type", "object")
        .put(
            "properties",
            JSONObject().put(
                "patches",
                JSONObject()
                    .put("type", "array")
                    .put("items", patchSchema),
            ),
        )
        .put("required", JSONArray().put("patches"))
        .put("additionalProperties", false)

    return JSONObject()
        .put("type", "json_schema")
        .put(
            "json_schema",
            JSONObject()
                .put("name", "live_translate_patches")
                .put("strict", true)
                .put("schema", schema),
        )
}

internal fun RealtimeTranslationClient.parseTranslationResponse(
    payload: String,
    request: TranslationRequest,
): TranslationResponse {
    if (payload.isBlank()) {
        throw IOException("Translation response payload was empty.")
    }
    try {
        val root = JSONObject(payload)
        val patchesJson = root.optJSONArray("patches")
            ?: throw IOException("Translation response did not include patches.")
        val patches = buildList {
            for (index in 0 until patchesJson.length()) {
                val patch = patchesJson.optJSONObject(index) ?: continue
                add(
                    TranslationPatch(
                        sourceStart = patch.optInt("sourceStart", Int.MIN_VALUE),
                        sourceEnd = patch.optInt("sourceEnd", Int.MIN_VALUE),
                        state = patch.optString("state"),
                        translation = patch.optString("translation"),
                    ),
                )
            }
        }
        return validateTranslationResponse(TranslationResponse(patches), request)
    } catch (error: JSONException) {
        throw IOException("Translation response was not valid JSON.", error)
    }
}

internal fun RealtimeTranslationClient.validateTranslationResponse(
    response: TranslationResponse,
    request: TranslationRequest,
): TranslationResponse {
    val expectedPatches = buildList<Triple<Int, Int, String>> {
        if (request.finalizedSource.isNotBlank()) {
            add(Triple(request.sourceStart, request.finalizedSourceEnd, "final"))
        }
        if (request.draftSource.isNotBlank()) {
            add(Triple(request.draftSourceStart, request.sourceEnd, "draft"))
        }
    }
    val normalized = mutableListOf<TranslationPatch>()
    expectedPatches.forEach { expected ->
        val patch = response.patches.firstOrNull { candidate ->
            candidate.sourceStart == expected.first &&
                candidate.sourceEnd == expected.second &&
                candidate.state == expected.third &&
                candidate.translation.isNotBlank()
        }
        if (patch != null) {
            normalized += patch.copy(translation = patch.translation.trim())
            return@forEach
        }

        if (expected.third == "draft" && !request.requiresDraftTranslation()) {
            normalized += TranslationPatch(
                sourceStart = expected.first,
                sourceEnd = expected.second,
                state = expected.third,
                translation = request.fallbackDraftTranslation(),
            )
            return@forEach
        }

        throw IOException(
            "Translation response missing expected ${expected.third} patch ${expected.first}-${expected.second}.",
        )
    }
    return TranslationResponse(normalized)
}
