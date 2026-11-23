package dev.screengoated.toolbox.mobile.componentupdate

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class ReleaseAssetUrlTest {
    private val asset = "help-index-v1-0123456789abcdef.json.gz"
    private val release = "https://github.com/nganlinh4/screen-goated-toolbox/releases/download"

    @Test
    fun acceptsOnlyTheExactAssetOfTheNamedRelease() {
        val tag = ReleaseAssetUrl.PRODUCTION_TAG
        assertTrue(ReleaseAssetUrl.matches("$release/$tag/$asset", tag, asset))
        listOf(
            "http://github.com/nganlinh4/screen-goated-toolbox/releases/download/$tag/$asset",
            "https://github.com.example/nganlinh4/screen-goated-toolbox/releases/download/$tag/$asset",
            "https://user@github.com/nganlinh4/screen-goated-toolbox/releases/download/$tag/$asset",
            "https://github.com:8443/nganlinh4/screen-goated-toolbox/releases/download/$tag/$asset",
            "$release/$tag/$asset?raw=1",
            "$release/$tag/$asset#x",
            "$release/${ReleaseAssetUrl.STAGING_TAG}/$asset",
            "$release/$tag/other-$asset",
            "$release/$tag/../$tag/$asset",
            "not a url",
        ).forEach { url -> assertFalse(url, ReleaseAssetUrl.matches(url, tag, asset)) }
    }

    /** Play base sources must pass the dex-string half of verifyPlayReleaseCompliance. */
    @Test
    fun playBaseSourcesAvoidEveryForbiddenDexString() {
        val module = moduleDir()
        val gate = File(module, "gradle/play-compliance.gradle.kts").readText()
        val block = gate.substringAfter("val forbiddenDexStrings = listOf(").substringBefore(")")
        val forbidden = Regex("\"([^\"]+)\"").findAll(block).map { it.groupValues[1] }.toList()
        assertTrue(forbidden.contains("github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles"))
        val offenders = listOf("src/main/java", "src/play/java")
            .map { File(module, it) }
            .flatMap { root -> root.walkTopDown().filter { it.extension in setOf("kt", "java") }.toList() }
            .flatMap { file ->
                val text = file.readText()
                forbidden.filter(text::contains).map { "${file.relativeTo(module)}: $it" }
            }
        assertEquals(emptyList<String>(), offenders)
    }

    private fun moduleDir(): File =
        generateSequence(File(requireNotNull(System.getProperty("user.dir"))).canonicalFile) { it.parentFile }
            .firstNotNullOf { dir ->
                listOf(dir, File(dir, "androidApp"), File(dir, "mobile/androidApp"))
                    .firstOrNull { File(it, "gradle/play-compliance.gradle.kts").isFile }
            }
}
