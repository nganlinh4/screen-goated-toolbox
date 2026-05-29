package dev.screengoated.toolbox.mobile.service

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class RealtimeTranslationClientParityTest {
    @Test
    fun `text llm chain uses shared provider availability gates`() {
        val clientSource = loadSourceFile(CLIENT_SOURCE_PATH).readText()
        val runtimeSource = loadSourceFile(RUNTIME_SOURCE_PATH).readText()

        assertTrue(clientSource.contains("import dev.screengoated.toolbox.mobile.preset.providerIsAvailable"))
        assertTrue(clientSource.contains("providerIsAvailable(descriptor.provider, apiKeys, runtimeSettings)"))
        assertTrue(runtimeSource.contains("runtimeSettings = repository.currentPresetRuntimeSettings()"))
    }

    private fun loadSourceFile(path: String): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, path) }
            .firstOrNull(File::exists)
            ?: error("Could not locate $path from $workingDirectory")
    }

    private companion object {
        private const val CLIENT_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiClients.kt"
        private const val RUNTIME_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/LiveSessionRuntime.kt"
    }
}
