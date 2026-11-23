package dev.screengoated.toolbox.mobile.phonecontrol.ui

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityConfiguredTest {
    @Test
    fun listedServiceMatchesFullAndShortForms() {
        val pkg = "dev.screengoated.toolbox.mobile"
        val cls = "dev.screengoated.toolbox.mobile.service.SgtAccessibilityService"
        assertTrue(accessibilityServiceListed("other/x.Y:$pkg/$cls", pkg, cls))
        assertTrue(accessibilityServiceListed("$pkg/.service.SgtAccessibilityService", pkg, cls))
        assertFalse(accessibilityServiceListed(null, pkg, cls))
        assertFalse(accessibilityServiceListed("", pkg, cls))
        assertFalse(accessibilityServiceListed("evil.pkg/$cls", pkg, cls))
    }

    @Test
    fun registryReusesTheSingleEnabledCheck() {
        val mainRoot = repoFile("mobile/androidApp/src/main/java")
        val parsers = mainRoot.walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .filter { it.readText().contains("ENABLED_ACCESSIBILITY_SERVICES") }
            .map { it.name }
            .toList()
        assertEquals(listOf("PhoneControlActivationFlow.kt"), parsers)
        val registry = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/capability/PhoneControlProviderRegistry.kt",
        ).readText()
        assertTrue(registry.contains("isAccessibilityConfigured(context)"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
