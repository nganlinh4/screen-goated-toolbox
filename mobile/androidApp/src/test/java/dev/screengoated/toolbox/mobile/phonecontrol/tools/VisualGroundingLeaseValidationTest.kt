package dev.screengoated.toolbox.mobile.phonecontrol.tools

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class VisualGroundingLeaseValidationTest {
    /**
     * `observe()` replaces the provider's published capture and action leases, so a one-element
     * lease check through it would discard the leases the model is acting on.
     */
    @Test
    fun leaseValidation_keepsPublishedLeases() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/tools/VisualGroundingToolHandlers.kt",
        ).readText()
        val validation = source
            .substringAfter("private suspend fun validateCurrentLease(")
            .substringBefore("\n    }\n")
        assertTrue(validation.contains("PhoneControlAccessibilityProvider.observeForVisual(maxElements = 1)"))
        assertFalse(validation.contains("PhoneControlAccessibilityProvider.observe("))

        val provider = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/provider/accessibility/PhoneControlAccessibilityProvider.kt",
        ).readText()
        assertTrue(
            provider.substringAfter("suspend fun observeForVisual(").substringBefore("\n    }\n")
                .contains("publishActionLeases = false"),
        )
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
