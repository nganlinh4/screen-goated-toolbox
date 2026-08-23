package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.componentupdate.verifyP256Signature
import java.security.MessageDigest
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONObject

internal enum class FeedReasoningControl {
    PLAIN,
    EFFORT_NONE,
    EFFORT_LOW,
    TEMPLATE_KWARGS,
    NO_THINK,
    THINKING_OFF,
}

internal data class FeedModel(
    val endpoint: String,
    val control: FeedReasoningControl?,
    val modality: String?,
    val p50Ms: Int?,
    val successRate: Double,
    val runs: Int,
)

internal data class AvailabilityFeed(
    val schemaVersion: Int,
    val controlVersion: Int,
    val availabilityGateVersion: Int,
    val provider: String,
    val generatedAt: String,
    val models: List<FeedModel>,
)

internal data class ModelFeedSnapshot(
    val feed: AvailabilityFeed? = null,
    val revision: Long = 0,
)

internal object PresetModelFeed {
    private val mutableState = MutableStateFlow(ModelFeedSnapshot())
    val state: StateFlow<ModelFeedSnapshot> = mutableState.asStateFlow()

    fun current(): AvailabilityFeed? = mutableState.value.feed

    fun publish(feed: AvailabilityFeed?) {
        mutableState.value = ModelFeedSnapshot(feed, mutableState.value.revision + 1)
    }

    fun rankedModels(): List<FeedModel> = rankedFeedModels(current())

    fun controlFor(provider: PresetModelProvider, fullName: String): FeedReasoningControl? {
        if (provider != PresetModelProvider.NVIDIA) return null
        return current()
            ?.takeIf { it.provider == "nvidia" }
            ?.models
            ?.firstOrNull { it.endpoint == fullName }
            ?.control
    }

    fun discoveredModels(): List<PresetModelDescriptor> {
        val feed = current() ?: return emptyList()
        return rankedFeedModels(feed).mapNotNull { model ->
            val type = feedModelType(model) ?: return@mapNotNull null
            if (knownEndpoint(feed.provider, model.endpoint)) return@mapNotNull null
            PresetModelDescriptor(
                id = discoveredModelId(feed.provider, model.endpoint),
                provider = PresetModelProvider.NVIDIA,
                fullName = model.endpoint,
                modelType = type,
                displayName = compactEndpointName(feed.provider, model.endpoint),
                quotaEn = "Unlimited",
                quotaVi = "Không giới hạn",
                quotaKo = "무제한",
                source = PresetModelSource.DISCOVERED,
                supportsSearchOverride = false,
                typicalLatencyMs = model.p50Ms,
                performanceSource = "availability-feed",
            )
        }
    }
}

internal fun parseVerifiedAvailabilityFeed(
    publicPoint: ByteArray,
    payload: ByteArray,
    rawSignature: ByteArray,
): AvailabilityFeed {
    require(payload.isNotEmpty() && payload.size <= MAXIMUM_FEED_BYTES)
    require(rawSignature.size == SIGNATURE_BYTES)
    require(verifyP256Signature(publicPoint, payload, rawSignature)) {
        "Availability feed signature is invalid"
    }
    val root = JSONObject(payload.toString(Charsets.UTF_8))
    val models = root.optJSONArray("models")?.let { array ->
        buildList {
            for (index in 0 until array.length()) {
                val item = array.getJSONObject(index)
                add(
                    FeedModel(
                        endpoint = item.getString("id"),
                        control = item.optString("control").takeIf(String::isNotBlank)
                            ?.let(::parseFeedControl),
                        modality = item.optString("modality").takeIf(String::isNotBlank),
                        p50Ms = item.optInt("p50_ms", -1).takeIf { it >= 0 },
                        successRate = item.optDouble("success_rate", 0.0),
                        runs = item.optInt("runs", 0),
                    ),
                )
            }
        }
    }.orEmpty()
    val feed = AvailabilityFeed(
        schemaVersion = root.getInt("schemaVersion"),
        controlVersion = root.optInt("controlVersion", CONTROL_VERSION),
        availabilityGateVersion = root.optInt("availabilityGateVersion", 0),
        provider = root.getString("provider"),
        generatedAt = root.getString("generatedAt"),
        models = models,
    )
    validateAvailabilityFeed(feed)
    return feed
}

internal fun validateAvailabilityFeed(feed: AvailabilityFeed) {
    require(feed.schemaVersion in SUPPORTED_SCHEMAS)
    require(feed.controlVersion == CONTROL_VERSION)
    require(feed.provider == "nvidia")
    if (feed.schemaVersion == 1) {
        require(feed.models.isEmpty())
    } else {
        require(feed.availabilityGateVersion == AVAILABILITY_GATE_VERSION)
    }
    feed.models.forEach { model ->
        require('/' in model.endpoint) { "Feed endpoint is not provider-qualified" }
        require(model.successRate in 0.0..1.0)
        require(model.runs >= 0)
        require(model.p50Ms == null || model.p50Ms >= 0)
    }
}

internal fun rankedFeedModels(feed: AvailabilityFeed?): List<FeedModel> = feed
    ?.models
    .orEmpty()
    .filter { it.successRate >= MINIMUM_SUCCESS_RATE && it.runs > 0 }
    .sortedWith(compareBy<FeedModel> { it.p50Ms ?: Int.MAX_VALUE }.thenBy { it.endpoint })

internal fun feedModelType(model: FeedModel): PresetModelType? {
    if (isDedicatedTranslationEndpoint(model.endpoint)) return null
    return when (model.modality) {
        null, "text" -> PresetModelType.TEXT
        "vision" -> PresetModelType.VISION
        else -> null
    }
}

internal fun discoveredModelId(provider: String, endpoint: String): String {
    val slug = endpoint.map { character ->
        if (character.isLetterOrDigit() && character.code < 128) character.lowercaseChar() else '-'
    }.joinToString("").trim('-').take(48).trimEnd('-').ifEmpty { "model" }
    val digest = MessageDigest.getInstance("SHA-256")
        .digest("$provider:$endpoint".toByteArray())
        .take(4)
        .joinToString("") { "%02x".format(it) }
    return "$provider-$slug-$digest"
}

internal fun compactEndpointName(provider: String, endpoint: String): String {
    val mark = provider.firstOrNull(Char::isLetterOrDigit)?.uppercaseChar() ?: '?'
    val initials = endpoint.substringAfterLast('/')
        .split(Regex("[^\\p{L}\\p{N}]+"))
        .filter(String::isNotEmpty)
        .take(8)
        .mapNotNull(String::firstOrNull)
        .joinToString("") { it.lowercase() }
        .ifEmpty { "model" }
    return "$mark $initials"
}

private fun knownEndpoint(provider: String, endpoint: String): Boolean =
    "$provider:$endpoint" in GeneratedPresetModelCatalogData.withdrawnEndpoints ||
        GeneratedPresetModelCatalogData.knownEndpoints.any {
            it.provider == PresetModelProvider.NVIDIA && it.fullName == endpoint
        }

private fun parseFeedControl(value: String): FeedReasoningControl = when (value) {
    "plain" -> FeedReasoningControl.PLAIN
    "effort-none" -> FeedReasoningControl.EFFORT_NONE
    "effort-low" -> FeedReasoningControl.EFFORT_LOW
    "template-kwargs" -> FeedReasoningControl.TEMPLATE_KWARGS
    "no-think" -> FeedReasoningControl.NO_THINK
    "thinking-off" -> FeedReasoningControl.THINKING_OFF
    else -> error("Unknown availability-feed control: $value")
}

private fun isDedicatedTranslationEndpoint(endpoint: String): Boolean = endpoint
    .lowercase()
    .split(Regex("[^a-z0-9]+"))
    .any { it in setOf("translate", "translation", "translator") }

internal fun decodeFeedPublicKey(value: String): ByteArray {
    require(value.length % 2 == 0 && value.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' })
    return ByteArray(value.length / 2) { index ->
        value.substring(index * 2, index * 2 + 2).toInt(16).toByte()
    }
}

internal const val MAXIMUM_FEED_BYTES = 256 * 1024
internal const val SIGNATURE_BYTES = 64
private val SUPPORTED_SCHEMAS = setOf(1, 3)
private const val CONTROL_VERSION = 1
private const val AVAILABILITY_GATE_VERSION = 1
private const val MINIMUM_SUCCESS_RATE = 0.8
