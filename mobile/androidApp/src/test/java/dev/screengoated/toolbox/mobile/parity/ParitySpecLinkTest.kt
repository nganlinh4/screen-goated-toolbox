package dev.screengoated.toolbox.mobile.parity

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class ParitySpecLinkTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `parity specs reference existing canonical files`() {
        val root = repoRoot()
        val parityDir = File(root, ".claude/parity")
        val markdownLinks = parityDir
            .listFiles { file -> file.extension == "md" && file.name != "feature-template.md" }
            .orEmpty()
            .flatMap { file ->
                windowsPathRegex.findAll(file.readText())
                    .map { match -> file to match.groupValues[1] }
                    .toList()
            }

        assertTrue("No parity spec source links found", markdownLinks.isNotEmpty())
        markdownLinks.forEach { (file, path) ->
            assertTrue(
                "${file.name} points at missing canonical file $path",
                File(root, path).exists(),
            )
        }
    }

    @Test
    fun `parity fixture canonical windows files exist`() {
        val root = repoRoot()
        val fixtures = File(root, "parity-fixtures")
            .walkTopDown()
            .filter { it.extension == "json" }
            .toList()
        val canonicalPaths = fixtures.flatMap { fixture ->
            val rootObject = json.parseToJsonElement(fixture.readText()).jsonObject
            rootObject["canonical_windows_files"]
                ?.jsonArray
                ?.map { fixture to it.jsonPrimitive.content }
                .orEmpty()
        }

        assertTrue("No fixture canonical Windows file lists found", canonicalPaths.isNotEmpty())
        canonicalPaths.forEach { (fixture, path) ->
            assertTrue(
                "${fixture.relativeTo(root)} points at missing canonical file $path",
                File(root, path).exists(),
            )
        }
    }

    @Test
    fun `preset model catalog is generated from shared manifest on both platforms`() {
        val root = repoRoot()
        val spec = File(root, ".claude/parity/preset-system.md").readText()
        val windowsBuild = File(root, "build.rs").readText()
        val androidBuild = File(root, "mobile/androidApp/build.gradle.kts").readText()
        val generator = File(root, "scripts/generate_android_preset_model_catalog.py").readText()

        assertTrue(spec.contains("[catalog/model_catalog.json]"))
        assertTrue(windowsBuild.contains("""manifest_dir.join("catalog/model_catalog.json")"""))
        assertTrue(windowsBuild.contains("""model_catalog_generated.rs"""))
        assertTrue(androidBuild.contains("""repoRoot.resolve("catalog/model_catalog.json")"""))
        assertTrue(androidBuild.contains("generatePresetModelCatalog"))
        assertTrue(androidBuild.contains("GeneratedPresetModelCatalogData.kt"))
        assertTrue(generator.contains("Generated from catalog/model_catalog.json. Do not edit by hand."))
    }

    private fun repoRoot(): File {
        val workingDir = requireNotNull(System.getProperty("user.dir"))
        var dir = File(workingDir).canonicalFile
        while (!File(dir, ".claude/parity").exists()) {
            dir = dir.parentFile?.canonicalFile
                ?: error("Could not find repo root from $workingDir")
        }
        return dir
    }

    private companion object {
        val windowsPathRegex = Regex("""\]\(\.\./\.\./([^)]+)\)""")
    }
}
