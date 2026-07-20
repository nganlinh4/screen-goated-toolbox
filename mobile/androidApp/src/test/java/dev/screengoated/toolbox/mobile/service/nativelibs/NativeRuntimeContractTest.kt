package dev.screengoated.toolbox.mobile.service.nativelibs

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class NativeRuntimeContractTest {
    @Test
    fun `shared manifest owns exact archives and flat members`() {
        val manifest = NativeRuntimeContract.parse(contractFile().readText())

        assertEquals("arm64-v8a", manifest.abi)
        assertEquals(setOf("ort", "moonshine", "sherpa"), manifest.archives.map { it.engine }.toSet())
        assertEquals("bundled_asset", manifest.archive("ort").fullDelivery)
        manifest.archives.forEach { archive ->
            assertEquals(archive.entries.size, archive.entries.map { it.fileName }.toSet().size)
            archive.entries.forEach { entry -> requireFlatLibraryName(entry.fileName) }
        }
    }

    @Test
    fun `manifest rejects undeclared fields and nested members`() {
        val valid = contractFile().readText()
        val extraField = valid.replaceFirst("\"schemaVersion\": 1", "\"schemaVersion\": 1, \"extra\": true")
        val nestedMember = valid.replaceFirst("libc++_shared.so", "nested/libc++_shared.so")

        assertThrows(IllegalArgumentException::class.java) {
            NativeRuntimeContract.parse(extraField)
        }
        assertThrows(IllegalArgumentException::class.java) {
            NativeRuntimeContract.parse(nestedMember)
        }
    }
}

internal fun repositoryRoot(): File {
    val start = File(requireNotNull(System.getProperty("user.dir"))).absoluteFile
    return generateSequence(start) { it.parentFile }
        .firstOrNull { File(it, "parity-fixtures/phone-control").isDirectory }
        ?: error("Could not find repository root from $start")
}

private fun contractFile(): File =
    File(repositoryRoot(), "parity-fixtures/phone-control/native-runtime-contract.json")
