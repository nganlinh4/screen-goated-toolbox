package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONObject

internal sealed interface CreationRuntimeStatus {
    data object Missing : CreationRuntimeStatus
    data class Downloading(val progress: Float) : CreationRuntimeStatus
    data class Ready(val sizeBytes: Long) : CreationRuntimeStatus
    data class Failed(val message: String) : CreationRuntimeStatus
}

internal class CreationRuntimeManager private constructor(context: Context) {
    private val applicationContext = context.applicationContext
    private val delivery = CreationRuntimeProvider(applicationContext)

    val status: StateFlow<CreationRuntimeStatus> = delivery.status

    fun startInstall() = delivery.startInstall()

    fun factory(): CreationRuntimeFactory? {
        // Play delivery can add this descriptor after the manager is constructed.
        val expected = loadCreationRuntimeProductDescriptor(applicationContext) ?: return null
        return delivery.factory()?.takeIf {
            isCompatibleCreationRuntimeManifest(it.runtimeManifest(), expected)
        }
    }

    suspend fun awaitFactory(): CreationRuntimeFactory? {
        factory()?.let { return it }
        delivery.startInstall()
        val terminal = withTimeoutOrNull(RUNTIME_WAIT_MS) {
            status.first {
                it is CreationRuntimeStatus.Ready || it is CreationRuntimeStatus.Failed
            }
        } ?: return null
        return if (terminal is CreationRuntimeStatus.Ready) factory() else null
    }

    fun delete() = delivery.delete()

    companion object {
        private const val RUNTIME_WAIT_MS = 5 * 60 * 1000L
        @Volatile private var instance: CreationRuntimeManager? = null

        fun get(context: Context): CreationRuntimeManager = instance ?: synchronized(this) {
            instance ?: CreationRuntimeManager(context).also { instance = it }
        }
    }
}

internal fun isCompatibleCreationRuntimeManifest(
    value: String,
    expected: CreationRuntimeProductDescriptor? = null,
): Boolean {
    val capabilities = parseCreationRuntimeCapabilities(value) ?: return false
    if (expected != null) {
        if (capabilities.runtimeVersion != expected.runtimeVersion ||
            capabilities.features != expected.features
        ) return false
    }
    return true
}

private data class CreationRuntimeCapabilities(
    val runtimeVersion: String,
    val features: Set<String>,
    val optionalInstruction: Map<String, Boolean>,
)

private fun parseCreationRuntimeCapabilities(value: String): CreationRuntimeCapabilities? =
    runCatching {
    require(value.encodeToByteArray().size <= MAXIMUM_RUNTIME_MANIFEST_BYTES)
    val manifest = JSONObject(value)
    require(
        manifest.keys().asSequence().toSet() ==
            setOf("contractVersion", "runtimeVersion", "features", "tools"),
    )
    val contractVersion = manifest.opt("contractVersion")
    require(
        (contractVersion is Int || contractVersion is Long) &&
            (contractVersion as Number).toLong() == 1L,
    )
    require(manifest.opt("runtimeVersion") is String)
    require(manifest.optString("runtimeVersion").isNotBlank())
    val featureValues = requireNotNull(manifest.optJSONArray("features"))
    require(featureValues.length() in 1..MAXIMUM_RUNTIME_FEATURES)
    val features = buildSet {
        for (index in 0 until featureValues.length()) {
            val feature = featureValues.getString(index).trim()
            require(feature.isNotEmpty() && add(feature))
        }
    }
    require(features == CREATION_RUNTIME_FEATURES)
    val tools = requireNotNull(manifest.optJSONObject("tools"))
    require(tools.keys().asSequence().toSet() == setOf(IMAGE_TO_3D_TOOL))
    val threeD = requireNotNull(tools.optJSONObject(IMAGE_TO_3D_TOOL))
    require(threeD.keys().asSequence().toSet() == setOf(GENERATION_MODES))
    val modes = requireNotNull(threeD.optJSONObject(GENERATION_MODES))
    require(modes.keys().asSequence().toSet() == CREATION_GENERATION_MODES)
    val optionalInstruction = CREATION_GENERATION_MODES.associateWith { mode ->
        val descriptor = requireNotNull(modes.optJSONObject(mode))
        require(descriptor.keys().asSequence().toSet() == setOf(OPTIONAL_INSTRUCTION))
        val capability = descriptor.opt(OPTIONAL_INSTRUCTION)
        require(capability is Boolean)
        capability
    }
    CreationRuntimeCapabilities(
        manifest.getString("runtimeVersion"),
        features,
        optionalInstruction,
    )
}.getOrNull()

internal fun runtimeSupportsOptionalInstruction(value: String, mode: String): Boolean =
    parseCreationRuntimeCapabilities(value)
        ?.optionalInstruction
        ?.get(mode) == true

private const val MAXIMUM_RUNTIME_FEATURES = 32
private const val IMAGE_TO_3D_TOOL = "image_to_3d"
private const val GENERATION_MODES = "generationModes"
private const val OPTIONAL_INSTRUCTION = "optionalInstruction"
private val CREATION_GENERATION_MODES = setOf("fast", "quality")
private val CREATION_RUNTIME_FEATURES =
    setOf("image_to_3d")
