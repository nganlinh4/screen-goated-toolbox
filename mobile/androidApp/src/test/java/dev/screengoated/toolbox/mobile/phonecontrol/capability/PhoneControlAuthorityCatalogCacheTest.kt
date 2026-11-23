package dev.screengoated.toolbox.mobile.phonecontrol.capability

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlAuthorityCatalogCacheTest {
    @Test
    fun providersFor_parsesAssetOnce() {
        val cache = PhoneControlAuthorityCatalogCache()
        var reads = 0
        val read = {
            reads += 1
            MATRIX
        }

        val first = cache.get(read)
        val second = cache.get(read)

        assertEquals(1, reads)
        assertSame(first, second)
        assertEquals(listOf("sgt_adb_bridge", "root_bridge"), first.routes.single().providerIds)
        assertEquals(false, first.providers.first().optional)
        assertEquals(true, first.providers.last().optional)
    }

    @Test
    fun canonicalMatrixParses() {
        val catalog = parsePhoneControlAuthorityCatalog(
            repoFile("parity-fixtures/phone-control/authority-matrix.json").readText(),
        )
        assertTrue(catalog.providers.isNotEmpty())
        assertTrue(catalog.routes.isNotEmpty())
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }

    private companion object {
        const val MATRIX = """{
            "providers": [
                {"id": "android_app_api", "authority": "app", "optional": false},
                {"id": "sgt_adb_bridge", "authority": "shell"}
            ],
            "routes": [
                {"capability": "command_execution", "providers": ["sgt_adb_bridge", "root_bridge"]}
            ]
        }"""
    }
}
