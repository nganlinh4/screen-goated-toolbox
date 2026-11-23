package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.PreparedFileTarget
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** Metadata frozen from the same retained target that will be committed. */
internal class PreparedResourceMutation(tool: String, target: PreparedFileTarget,
                                        sourceMetadata: JsonObject = JsonObject(emptyMap())) {
    val targetLease = target.lease
    private val base = target.proposal()
    init {
        require(tool in setOf("edit_text_file", "edit_text_file_structure", "save_artifact"))
        require(sourceMetadata.keys.intersect(base.keys).isEmpty())
        require(tool == "save_artifact" || targetLease.existedBefore)
    }
    val proposal = JsonObject(base + sourceMetadata + if (tool == "save_artifact") emptyMap() else mapOf(
        "operation" to JsonPrimitive("modify_existing_text_file"),
    ))
}
