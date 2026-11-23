package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.preset.ui.withCustomName
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class PresetCustomNameTest {
    @Test
    fun renameInVietnamese_updatesNameVi() {
        val renamed = DefaultPresets.all.first().withCustomName("Dịch nhanh")
        for (lang in listOf("en", "vi", "ko")) {
            assertEquals(lang, "Dịch nhanh", renamed.name(lang))
        }
    }

    @Test
    fun editorRenameNeverWritesOnlyTheEnglishName() {
        val editor = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset/ui/PresetEditorScreen.kt",
        ).readText()
        assertFalse(editor.contains("copy(nameEn ="))
        assertFalse(editor.contains("onNameChanged"))
        assertFalse(editor.contains("PresetTextDraft(editState.nameEn"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
