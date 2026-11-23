package dev.screengoated.toolbox.mobile

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Release signing credentials come from the environment, never from tracked source. */
class ReleaseSigningSourceTest {
    @Test
    fun gradleSigningHasNoLiteralCredentials() {
        val gradle = repoFile("mobile/androidApp/build.gradle.kts").readText()
        val literalSecret = Regex("""(storePassword|keyPassword)\s*=\s*"""")
        assertFalse(literalSecret.containsMatchIn(gradle))
        assertFalse(gradle.contains("screengoated\""))
        assertTrue(gradle.contains("\"SGT_RELEASE_STORE_PASSWORD\""))
        assertTrue(gradle.contains("\"SGT_RELEASE_KEY_PASSWORD\""))
    }

    @Test
    fun missingCredentialsFailSignedPackagingInsteadOfEmittingUnsignedArtifacts() {
        val gradle = repoFile("mobile/androidApp/build.gradle.kts").readText()
        val guard = gradle.substringAfter("if (!releaseSigning.ready) {").substringBefore("\n}\n")
        val taskPattern = Regex(guard.substringAfter("Regex(\"").substringBefore("\")"))
        listOf("packageFullRelease", "packagePlayRelease", "packagePlayReleaseBundle", "signPlayReleaseBundle")
            .forEach { assertTrue(it, taskPattern.matches(it)) }
        listOf("packageFullDebug", "packagePlayBenchmarkRelease", "packageFullNonMinifiedRelease")
            .forEach { assertFalse(it, taskPattern.matches(it)) }
        assertTrue(guard.contains("doFirst"))
    }

    @Test
    fun releaseScriptNeverPrintsPasswords() {
        val script = repoFile("mobile/build-release.ps1").readText()
        assertFalse(script.contains("-storepass"))
        assertFalse(script.contains("-keypass"))
        assertFalse(script.contains("screengoated -"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
