package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class CreationCompanionParityTest {
    @get:Rule val directory = TemporaryFolder()

    private fun request(operation: String) = CreationWorkerRequest(
        jobId = "job", tool = "3d", operation = operation, imagePath = "input.png",
        outputPath = File(directory.root, "model.glb").path, outputName = "model.glb",
    )

    private fun companion(name: String = "model.fbx"): CreationWorkerEvent {
        val file = directory.newFile(name)
        file.writeBytes("Kaydara FBX Binary  \u0000".toByteArray(Charsets.US_ASCII))
        return CreationWorkerEvent(event = "success", downloadPath = file.path, downloadName = name)
    }

    @Test fun everyModelRevisionPublishesItsValidatedCompanion() {
        val fixture = generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it, "parity-fixtures/image-to-3d/android-state.json") }.first(File::isFile)
        val operations = JSONObject(fixture.readText()).getJSONArray("companionOperations")
        val event = companion()
        for (index in 0 until operations.length()) {
            assertEquals(File(event.downloadPath!!), validatedCreationCompanion(request(operations.getString(index)), event))
        }
    }

    @Test fun separationStillRejectsAnUnrelatedSibling() {
        val event = companion("other.fbx")
        assertThrows(IllegalArgumentException::class.java) { validatedCreationCompanion(request("segment"), event) }
    }

    @Test fun refinementStillRejectsAnInvalidHeader() {
        val event = companion()
        File(event.downloadPath!!).writeBytes(ByteArray(21))
        assertThrows(IllegalArgumentException::class.java) { validatedCreationCompanion(request("refine"), event) }
    }

    @Test fun anotherToolCannotPublishAModelCompanion() {
        val event = companion()
        assertThrows(IllegalArgumentException::class.java) {
            validatedCreationCompanion(request("generate").copy(tool = "svg"), event)
        }
    }
}
