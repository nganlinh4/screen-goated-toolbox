package dev.screengoated.toolbox.mobile.phonecontrol.provider

import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PreparedResourceMutation
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FilePublication
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.PreparedFileTarget
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull

internal class PreparedArtifactSave(
    private val artifact: PhoneControlArtifact,
    private val target: PreparedFileTarget,
    private val overwrite: Boolean,
    private val admitSource: () -> Boolean,
) : AutoCloseable {
    private val bytes = artifact.bytes
    val mutation = PreparedResourceMutation("save_artifact", target, buildJsonObject {
        put("source_artifact_id", artifact.id)
        put("source_sha256", artifact.sha256)
        put("source_byte_count", bytes.size)
        put("overwrite_requested", overwrite)
    })

    fun commit(owner: PhoneControlEffectOwner, stillAuthorized: () -> Boolean): AndroidProviderResult =
        when (val result = FilePublication.commit(target, bytes, artifact.sha256, owner, stillAuthorized, admitSource)) {
            is AndroidProviderResult.Failure -> when {
                !target.lease.existedBefore && overwrite && result.code == "path_exists" -> result.copy(code = "ERR_FILE_TARGET_LEASE_CHANGED")
                !overwrite && result.code == "ERR_FILE_TARGET_LEASE_CHANGED" &&
                    (result.data["target_appeared"] as? JsonPrimitive)?.booleanOrNull == true -> result.copy(code = "path_exists")
                else -> result
            }
            is AndroidProviderResult.Success -> result.copy(data = JsonObject(result.data + buildJsonObject {
                put("artifact_id", artifact.id)
                put("source_artifact_id", artifact.id)
                put("source_sha256", artifact.sha256)
            }))
        }

    override fun close() = target.close()
}
