package dev.screengoated.toolbox.mobile.ui

import androidx.compose.ui.text.input.KeyboardType
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CredentialKeyboardOptionsTest {
    @Test
    fun credentialField_usesPasswordKeyboardWithoutAutocorrect() {
        assertEquals(KeyboardType.Password, CredentialKeyboardOptions.keyboardType)
        assertEquals(false, CredentialKeyboardOptions.autoCorrectEnabled)

        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/MobileShellCards.kt",
        ).readText()
        assertTrue(source.contains("keyboardOptions = if (entry.isPassword) CredentialKeyboardOptions"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
