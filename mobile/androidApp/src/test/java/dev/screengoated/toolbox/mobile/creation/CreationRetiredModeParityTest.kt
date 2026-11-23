package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class CreationRetiredModeParityTest {
    private fun fixture(): JSONObject = JSONObject(
        generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it, "parity-fixtures/image-to-3d/input-contract.json") }
            .first(File::isFile)
            .readText(),
    )

    @Test
    fun retiredModesFailPreflightLikeWindows() {
        val rows = fixture().getJSONArray("retiredModes")
        for (index in 0 until rows.length()) {
            val row = rows.getJSONObject(index)
            assertEquals(
                row.toString(),
                row.getString("error"),
                creationInputError(
                    CreationTool.IMAGE_TO_3D,
                    row.getString("mode"),
                    row.getInt("width"),
                    row.getInt("height"),
                    row.getLong("bytes"),
                ),
            )
        }
    }
}
