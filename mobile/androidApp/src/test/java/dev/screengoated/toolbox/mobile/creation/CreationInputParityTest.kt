package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import dev.screengoated.toolbox.mobile.creation.runtime.isCompatibleCreationRuntimeManifest
import dev.screengoated.toolbox.mobile.creation.runtime.runtimeGenerationModes

class CreationInputParityTest {
    private fun fixture(path: String): JSONObject = JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
        .map { File(it, path) }.first(File::isFile).readText())

    @Test fun sharedInputBoundaries() {
        for ((tool, path) in listOf(CreationTool.IMAGE_TO_3D to "image-to-3d", CreationTool.IMAGE_TO_SVG to "image-to-svg")) {
            val cases = fixture("parity-fixtures/$path/input-contract.json").getJSONArray("cases")
            for (index in 0 until cases.length()) {
                val row = cases.getJSONObject(index)
                assertEquals(row.toString(), if (row.isNull("error")) null else row.getString("error"),
                    creationInputError(tool, row.optString("mode", "quality"), row.getInt("width"), row.getInt("height"), row.getLong("bytes")))
            }
        }
    }

    @Test fun topologyAndWaitingStates() {
        val data = fixture("parity-fixtures/image-to-3d/android-state.json")
        val stages = data.getJSONArray("activeStages")
        for (index in 0 until stages.length()) {
            assertEquals(CreationNativeStage.RUNNING, CreationJobStatus(stage = stages.getString(index), progressText = "").toNativeStage())
        }
        val cases = data.getJSONArray("topologyCases")
        for (index in 0 until cases.length()) {
            val row = cases.getJSONObject(index)
            assertEquals(row.getString("expected"), CreationContract.initialTopology(CreationGenerationMode.fromWireName(row.getString("mode")), row.getBoolean("autoSegment"), row.getString("topology")))
        }
    }

    @Test fun partialGenerationCapabilitiesDoNotInvalidateTheRuntime() {
        val cases = fixture("parity-fixtures/image-to-3d/android-state.json").getJSONArray("capabilityModes")
        for (index in 0 until cases.length()) {
            val row = cases.getJSONObject(index)
            val modes = row.getJSONArray("modes")
            val entries = JSONObject()
            for (mode in 0 until modes.length()) {
                entries.put(modes.getString(mode), JSONObject().put("optionalInstruction", false))
            }
            val manifest = JSONObject().put("contractVersion", 1).put("runtimeVersion", "1")
                .put("features", org.json.JSONArray(listOf("image_to_3d", "image_to_svg", "image_creator")))
                .put("tools", JSONObject().put("image_to_3d", JSONObject().put("generationModes", entries))).toString()
            assertEquals(row.toString(), row.getBoolean("compatible"), isCompatibleCreationRuntimeManifest(manifest))
            if (row.getBoolean("compatible")) {
                assertEquals((0 until modes.length()).map(modes::getString).toSet(), runtimeGenerationModes(manifest))
            }
        }
    }
}
