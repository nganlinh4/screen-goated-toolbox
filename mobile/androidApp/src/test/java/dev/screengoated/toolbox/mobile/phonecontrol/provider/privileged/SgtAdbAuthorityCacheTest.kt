package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.File
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SgtAdbAuthorityCacheTest {
    private val ready = buildJsonObject { put("state", JsonPrimitive("ready")) }

    @Test
    fun verifiedProbeIsReusedOnlyForTheSameTransportWithinTheAgeBound() {
        val cache = SgtAdbAuthorityCache(maxAgeMs = 1_000L)
        assertNull(cache.current(connectionGeneration = 7L, nowMs = 0L))
        cache.remember(connectionGeneration = 7L, nowMs = 100L, probe = ready)
        assertEquals(ready, cache.current(7L, 100L))
        assertEquals(ready, cache.current(7L, 1_099L))
        assertNull(cache.current(7L, 1_100L))
        assertNull(cache.current(8L, 200L))
        assertNull(cache.current(7L, 99L))
        cache.clear()
        assertNull(cache.current(7L, 200L))
    }

    @Test
    fun everyTransportChangeMintsANewGenerationAndOnlyReadyProbesAreCached() {
        val manager = source("SgtAdbConnectionManager.kt")
        val connect = manager.substringAfter("private suspend fun connectDiscoveredUntil(")
            .substringBefore("fun disconnectPreservingKey()")
        assertTrue(
            connect.indexOf("generation.set(nextSgtAdbConnectionGeneration())") <
                connect.indexOf("connect(host, endpoint.port)"),
        )
        val disconnect = manager.substringAfter("fun disconnectPreservingKey()")
            .substringBefore("\n    }")
        assertTrue(disconnect.contains("generation.set(nextSgtAdbConnectionGeneration())"))

        val service = source("SgtAdbBridgeService.kt")
        val cached = service.substringAfter("private fun cachedOrVerifiedAuthority(")
            .substringBefore("private fun verifyAuthority(")
        assertTrue(cached.indexOf("manager().connectionGeneration") < cached.indexOf("verifyAuthority("))
        assertTrue(cached.contains("== \"ready\""))
        assertTrue(cached.contains("authorityCache.clear()"))
        val forget = service.substringAfter("override fun forget()").substringBefore("synchronized(lock)")
        assertTrue(forget.contains("authorityCache.clear()"))
    }

    private fun source(name: String): String {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/" +
                "provider/privileged/$name",
        ).readText()
    }
}
