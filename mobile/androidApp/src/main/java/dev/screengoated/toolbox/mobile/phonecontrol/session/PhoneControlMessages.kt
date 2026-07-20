package dev.screengoated.toolbox.mobile.phonecontrol.session

import android.content.Context
import android.graphics.Bitmap
import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveMediaResolution
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSetupSpec
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveTranscriptionMode
import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import dev.screengoated.toolbox.mobile.shared.live.buildGeminiLiveSetup
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.io.ByteArrayOutputStream
import java.util.Base64
import kotlin.math.min
import kotlin.math.sqrt

internal data class PhoneControlContractAssets(
    val functionDeclarations: JsonArray,
    val canonicalPrompt: String,
) {
    companion object {
        fun load(context: Context, json: Json): PhoneControlContractAssets {
            val catalog = context.assets.open(GeneratedPhoneControlContract.CATALOG_ASSET_PATH)
                .bufferedReader()
                .use { json.parseToJsonElement(it.readText()) as JsonObject }
            val declarations = catalog["functionDeclarations"] as? JsonArray
                ?: error("Phone Control catalog has no functionDeclarations array")
            check(declarations.size == GeneratedPhoneControlContract.STATIC_DECLARATION_COUNT) {
                "Phone Control catalog declaration count mismatch"
            }
            val prompt = context.assets.open(GeneratedPhoneControlContract.PROMPT_CORE_ASSET_PATH)
                .bufferedReader()
                .use { it.readText() }
            check(GeneratedPhoneControlContract.PLATFORM_DEVICE_TOKEN in prompt) {
                "Canonical Phone Control prompt lost platform token"
            }
            return PhoneControlContractAssets(declarations, prompt)
        }
    }
}

internal fun buildPhoneControlSetupPayload(
    assets: PhoneControlContractAssets,
    capabilityContext: String,
    voiceName: String,
    resumptionHandle: String? = null,
): String {
    val instruction = buildString {
        append(
            assets.canonicalPrompt.replace(
                GeneratedPhoneControlContract.PLATFORM_DEVICE_TOKEN,
                "Android phone or tablet",
            ),
        )
        append("\n\nANDROID PROVIDER CONTRACT\n")
        append("Use the declared tool unchanged. Each result reports the provider, its state, ")
        append("effect certainty, observation generation, and any required user step. ")
        append("A declared tool can return capability_unavailable; never invent success or silently ")
        append("substitute a different requested effect. System-owned confirmations and credential ")
        append("surfaces remain user-owned. Accessibility targets are valid only for their exact ")
        append("observation generation.\n")
        append("For Android type_text and key_combination, pass the current observation-bound ")
        append("surface token returned by list_windows; a snapshot-local node @id is not a ")
        append("surface target. type_text and editor key chords resolve the one focused editable ")
        append("node inside that exact live surface. key_combination also supports the exact ")
        append("single system keys back, home, recents, notifications, and quick_settings without ")
        append("requiring an editor. paste_artifact resolves the current unique focused editor ")
        append("without exposing artifact text to the model.\n")
        append(capabilityContext)
    }
    val tools = buildJsonArray {
        add(buildJsonObject { put("functionDeclarations", assets.functionDeclarations) })
    }
    return buildGeminiLiveSetup(
        GeminiLiveSetupSpec(
            apiModel = GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1,
            mediaResolution = GeminiLiveMediaResolution.HIGH,
            voiceName = voiceName.ifBlank { "Aoede" },
            systemInstruction = instruction,
            transcriptionMode = GeminiLiveTranscriptionMode.BOTH,
            contextWindowCompression = true,
            generationOverrides = buildJsonObject {
                put("thinkingConfig", buildJsonObject { put("includeThoughts", true) })
            },
            setupExtensions = buildJsonObject {
                put("tools", tools)
                put(
                    "sessionResumption",
                    buildJsonObject {
                        resumptionHandle
                            ?.takeIf(String::isNotBlank)
                            ?.let { put("handle", it) }
                    },
                )
                put(
                    "realtimeInputConfig",
                    buildJsonObject { put("activityHandling", "START_OF_ACTIVITY_INTERRUPTS") },
                )
            },
        ),
    ).toString()
}

internal fun buildPhoneControlAudioPayload(samples: ShortArray): String {
    val bytes = ByteArray(samples.size * 2)
    samples.forEachIndexed { index, sample ->
        bytes[index * 2] = (sample.toInt() and 0xFF).toByte()
        bytes[index * 2 + 1] = ((sample.toInt() ushr 8) and 0xFF).toByte()
    }
    return buildJsonObject {
        put(
            "realtimeInput",
            buildJsonObject {
                put(
                    "audio",
                    buildJsonObject {
                        put("data", Base64.getEncoder().encodeToString(bytes))
                        put("mimeType", "audio/pcm;rate=16000")
                    },
                )
            },
        )
    }.toString()
}

internal fun buildPhoneControlScreenPayload(bitmap: Bitmap): String {
    return buildPhoneControlScreenPayload(encodePhoneControlScreenImage(bitmap))
}

internal fun encodePhoneControlScreenImage(bitmap: Bitmap): ByteArray {
    var ownedBitmap: Bitmap? = null
    var candidate = bitmap
    try {
        val scale = boundedBitmapScale(bitmap.width, bitmap.height)
        if (scale < 1f) {
            candidate = Bitmap.createScaledBitmap(
                bitmap,
                (bitmap.width * scale).toInt().coerceAtLeast(1),
                (bitmap.height * scale).toInt().coerceAtLeast(1),
                true,
            )
            ownedBitmap = candidate
        }
        return encodeBoundedJpeg(candidate)
    } finally {
        ownedBitmap?.recycle()
    }
}

internal fun buildPhoneControlScreenPayload(encodedJpeg: ByteArray): String = buildJsonObject {
    put(
        "realtimeInput",
        buildJsonObject {
            put(
                "video",
                buildJsonObject {
                    put("data", Base64.getEncoder().encodeToString(encodedJpeg))
                    put("mimeType", "image/jpeg")
                },
            )
        },
    )
}.toString()

internal fun buildPhoneControlTextPayload(text: String): String = buildJsonObject {
    put("realtimeInput", buildJsonObject { put("text", text) })
}.toString()

internal fun buildPhoneControlToolResponse(
    id: String,
    name: String,
    response: JsonObject,
): String = buildJsonObject {
    put(
        "toolResponse",
        buildJsonObject {
            put(
                "functionResponses",
                buildJsonArray {
                    add(
                        buildJsonObject {
                            put("id", id)
                            put("name", name)
                            put("response", response)
                        },
                    )
                },
            )
        },
    )
}.toString()

internal fun buildPhoneControlActivityStartPayload(): String = buildJsonObject {
    put("realtimeInput", buildJsonObject { put("activityStart", JsonPrimitive(true)) })
}.toString()

internal fun buildPhoneControlActivityEndPayload(): String = buildJsonObject {
    put("realtimeInput", buildJsonObject { put("activityEnd", JsonPrimitive(true)) })
}.toString()

private fun boundedBitmapScale(width: Int, height: Int): Float {
    require(width > 0 && height > 0) { "Screen frame dimensions must be positive" }
    val dimensionScale = MAX_SCREEN_DIMENSION.toFloat() / maxOf(width, height)
    val pixelScale = sqrt(MAX_SCREEN_PIXELS.toDouble() / (width.toDouble() * height)).toFloat()
    return min(1f, min(dimensionScale, pixelScale))
}

private fun encodeBoundedJpeg(bitmap: Bitmap): ByteArray {
    val output = ByteArrayOutputStream()
    var quality = SCREEN_JPEG_QUALITY
    while (quality >= MIN_SCREEN_JPEG_QUALITY) {
        output.reset()
        check(bitmap.compress(Bitmap.CompressFormat.JPEG, quality, output)) {
            "Could not encode Phone Control screen frame"
        }
        if (output.size() <= MAX_SCREEN_JPEG_BYTES) return output.toByteArray()
        quality -= SCREEN_JPEG_QUALITY_STEP
    }
    error("Phone Control screen frame exceeded the encoded-size limit")
}

private const val MAX_SCREEN_DIMENSION = 1_280
private const val MAX_SCREEN_PIXELS = 1_200_000
private const val MAX_SCREEN_JPEG_BYTES = 640 * 1_024
private const val SCREEN_JPEG_QUALITY = 82
private const val MIN_SCREEN_JPEG_QUALITY = 42
private const val SCREEN_JPEG_QUALITY_STEP = 10
