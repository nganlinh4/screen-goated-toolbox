package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationRuntimeCapabilityTickTest {
    @Test
    fun oneTickReadsTheRuntimeOncePerModeNotOncePerItem() {
        val reads = mutableListOf<String>()
        val supports = creationMemoizedInstructionSupport { mode ->
            reads += mode
            mode == CreationGenerationMode.QUALITY.wireName
        }
        val items = (0 until 50).map { index ->
            CreationNativeItem(
                id = "item_$index",
                batchId = "batch",
                sourcePath = "creation/sources/$index.png",
                sourceName = "$index.png",
                generationMode = if (index % 2 == 0) {
                    CreationGenerationMode.QUALITY.wireName
                } else {
                    CreationGenerationMode.FAST.wireName
                },
            )
        }
        val applied = items.map { applyCreationRuntimeCapabilities(CreationTool.IMAGE_TO_3D, it, supports) }
        assertEquals(2, reads.size)
        assertEquals(reads.toSet().size, reads.size)
        assertTrue(applied.filter { it.generationMode == CreationGenerationMode.QUALITY.wireName }.all { it.allowsInstruction })
        assertFalse(applied.filter { it.generationMode == CreationGenerationMode.FAST.wireName }.any { it.allowsInstruction })
    }

    @Test
    fun statusLoopsReadCapabilitiesOutsideTheUpdateLambda() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        val viewModel = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/CreationNativeViewModel.kt",
        ).readText()
        assertFalse(viewModel.contains("map(::withRuntimeCapabilities)"))
        val lambdas = updateLambdaBodies(viewModel)
        assertTrue(lambdas.size >= 5)
        lambdas.forEach { update ->
            assertFalse(update, update.contains("manager.preparationStatus("))
            assertFalse(update, update.contains("supportsOptionalInstruction"))
        }
    }

    /** Brace-matched body of every `mutableState.update { ... }` lambda. */
    private fun updateLambdaBodies(source: String): List<String> {
        val opener = "mutableState.update {"
        val bodies = mutableListOf<String>()
        var start = source.indexOf(opener)
        while (start >= 0) {
            val open = start + opener.length - 1
            var depth = 0
            var end = open
            while (end < source.length) {
                when (source[end]) {
                    '{' -> depth += 1
                    '}' -> if (--depth == 0) break
                }
                end += 1
            }
            bodies += source.substring(open, end + 1)
            start = source.indexOf(opener, end)
        }
        return bodies
    }
}
