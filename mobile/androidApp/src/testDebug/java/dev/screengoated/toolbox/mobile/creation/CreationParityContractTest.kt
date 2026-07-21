package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.creation.worker.freshGeneratedSvg
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class CreationParityContractTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `image to 3d Android contract matches shared fixture`() {
        val fixture = fixture("image-to-3d")
        val limits = fixture["limits"]!!.jsonObject
        val defaults = fixture["defaults"]!!.jsonObject
        val names = fixture["names"]!!.jsonObject

        assertEquals(CreationContract.MINIMUM_POLYCOUNT, limits.int("minimumPolycount"))
        assertEquals(CreationContract.MAXIMUM_POLYCOUNT, limits.int("maximumPolycount"))
        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.int("maximumParallelJobs"))
        assertEquals(
            CreationContract.MAXIMUM_CONCURRENT_PREPARATIONS,
            limits.int("maximumConcurrentPreparations"),
        )
        assertEquals(
            CreationContract.MINIMUM_PREPARATION_INTERVAL_SECONDS,
            limits.int("minimumPreparationIntervalSeconds"),
        )
        assertEquals(CreationContract.IMAGE_TO_3D_WORKSPACES, limits.int("preparedWorkspaces"))
        assertEquals(CreationContract.DEFAULT_POLYCOUNT, defaults.int("polycount"))
        assertEquals(names.string("en"), MobileLocaleText.forLanguage("en").appImageTo3dTitle)
        assertEquals(names.string("ko"), MobileLocaleText.forLanguage("ko").appImageTo3dTitle)
        assertEquals(names.string("vi"), MobileLocaleText.forLanguage("vi").appImageTo3dTitle)
    }

    @Test
    fun `image to SVG Android contract matches shared fixture`() {
        val fixture = fixture("image-to-svg")
        val limits = fixture["limits"]!!.jsonObject
        val models = fixture["models"]!!.jsonObject

        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.int("maximumParallelJobs"))
        assertEquals(
            CreationContract.MAXIMUM_CONCURRENT_PREPARATIONS,
            limits.int("maximumConcurrentPreparations"),
        )
        assertEquals(
            CreationContract.MINIMUM_PREPARATION_INTERVAL_SECONDS,
            limits.int("minimumPreparationIntervalSeconds"),
        )
        assertEquals(CreationContract.IMAGE_TO_SVG_WORKSPACES, limits.int("preparedWorkspaces"))
        assertEquals(CreationContract.SVG_MINIMUM_REUSABLE_CREDITS, limits.int("minimumReusableCredits"))
        assertEquals(CreationContract.svgRemoteModel("simple"), models.modelString("simple", "remoteModel"))
        assertEquals(CreationContract.svgCreditCost("simple"), models.modelInt("simple", "creditCost"))
        assertEquals(CreationContract.svgRemoteModel("detail"), models.modelString("detail", "remoteModel"))
        assertEquals(CreationContract.svgCreditCost("detail"), models.modelInt("detail", "creditCost"))
    }

    @Test
    fun `new SVG selection never substitutes an old dashboard result`() {
        val old = listOf("old-a", "old-b")
        assertNull(freshGeneratedSvg(old, old))
        assertEquals("new", freshGeneratedSvg(listOf("new", "old-a", "old-b"), old))
        assertEquals("same", freshGeneratedSvg(listOf("same", "same"), listOf("same")))
    }

    private fun fixture(tool: String) = json.parseToJsonElement(
        File(repoRoot(), "parity-fixtures/$tool/state-contract.json").readText(),
    ).jsonObject

    private fun repoRoot(): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = directory.parentFile?.canonicalFile ?: error("Could not find repository root")
        }
        return directory
    }
}

private fun kotlinx.serialization.json.JsonObject.int(key: String): Int =
    this[key]!!.jsonPrimitive.int

private fun kotlinx.serialization.json.JsonObject.string(key: String): String =
    this[key]!!.jsonPrimitive.content

private fun kotlinx.serialization.json.JsonObject.modelString(model: String, key: String): String =
    this[model]!!.jsonObject.string(key)

private fun kotlinx.serialization.json.JsonObject.modelInt(model: String, key: String): Int =
    this[model]!!.jsonObject.int(key)
