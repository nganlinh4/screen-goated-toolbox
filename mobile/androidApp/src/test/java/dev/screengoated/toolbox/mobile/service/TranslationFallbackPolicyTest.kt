package dev.screengoated.toolbox.mobile.service

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Paths

class TranslationFallbackPolicyTest {
    @Test
    fun staleRequestCannotStartFallback() = runBlocking {
        var current = true
        var fallbackCalls = 0
        assertThrows(StaleTranslationRequestException::class.java) {
            runBlocking {
                withTranslationFallback<String>({ current }, primary = {
                    current = false
                    error("provider failed")
                }, fallback = { fallbackCalls++; "fallback" })
            }
        }
        assertEquals(0, fallbackCalls)
    }

    @Test
    fun cancellationCannotStartFallback() {
        var fallbackCalls = 0
        assertThrows(CancellationException::class.java) {
            runBlocking {
                withTranslationFallback<String>({ true }, primary = {
                    throw CancellationException("cancelled")
                }, fallback = { fallbackCalls++; "fallback" })
            }
        }
        assertEquals(0, fallbackCalls)
    }

    @Test
    fun currentRequestCanUseFallback() = runBlocking {
        val result = withTranslationFallback({ true }, primary = { error("provider failed") }, fallback = { "fallback" })
        assertEquals("fallback", result)
    }

    @Test
    fun sharedFallbackCasesRouteOnlyCurrentRequests() = runBlocking {
        val paths = listOf(
            Paths.get("..", "parity-fixtures", "live-translate", "translation-request-identity.json"),
            Paths.get("..", "..", "parity-fixtures", "live-translate", "translation-request-identity.json"),
            Paths.get("parity-fixtures", "live-translate", "translation-request-identity.json"),
        )
        val fixture = Json.parseToJsonElement(Files.readAllBytes(paths.first { Files.exists(it) }).decodeToString()).jsonObject
        fixture.getValue("fallbackCases").jsonArray.forEach { item ->
            val case = item.jsonObject
            val selected = case.getValue("selected").jsonPrimitive.content
            var attempted: String? = null
            try {
                withTranslationFallback({ case.getValue("requestCurrent").jsonPrimitive.boolean }, primary = {
                    error("primary failed")
                }, fallback = {
                    attempted = fallbackTranslationProviderId(selected)
                    if (case["fallbackSucceeds"]?.jsonPrimitive?.boolean == false) error("fallback failed")
                    "translated"
                })
            } catch (_: StaleTranslationRequestException) {
                // A retired request cannot start the alternate provider.
            } catch (_: IllegalStateException) {
                // The alternate failed; the preference still belongs to the user.
            }
            assertEquals(case.getValue("name").jsonPrimitive.content, case["fallback"]?.jsonPrimitive?.contentOrNull, attempted)
        }
    }
}
