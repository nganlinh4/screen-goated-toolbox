package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.nio.charset.StandardCharsets
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SgtAdbBrowserTunnelTest {
    @Test
    fun authenticatedHeaderIsRequiredExactlyOnceAndStrippedBeforeChrome() {
        val token = AuthenticatedBrowserTunnel.newBearerToken()
        val accepted = request(token)
        val sanitized = AuthenticatedBrowserTunnel.sanitizedAuthenticatedHeader(accepted, token)
            ?.toString(StandardCharsets.ISO_8859_1)
            ?: error("valid request was rejected")

        assertTrue(sanitized.startsWith("GET /json/list HTTP/1.1\r\n"))
        assertTrue(sanitized.contains("Host: 127.0.0.1\r\n"))
        assertFalse(sanitized.contains(token))
        assertFalse(sanitized.contains("X-SGT-Bridge-Token", ignoreCase = true))

        assertNull(
            AuthenticatedBrowserTunnel.sanitizedAuthenticatedHeader(
                request("wrong-token"),
                token,
            ),
        )
        assertNull(
            AuthenticatedBrowserTunnel.sanitizedAuthenticatedHeader(
                request("$token\r\nX-SGT-Bridge-Token: $token"),
                token,
            ),
        )
    }

    @Test
    fun bearerTokensAreStrongAndNeverStable() {
        val first = AuthenticatedBrowserTunnel.newBearerToken()
        val second = AuthenticatedBrowserTunnel.newBearerToken()

        assertTrue(first.length >= 40)
        assertNotEquals(first, second)
        assertTrue(AuthenticatedBrowserTunnel.constantTimeEquals(first, first))
        assertFalse(AuthenticatedBrowserTunnel.constantTimeEquals(first, second))
    }

    private fun request(token: String): ByteArray =
        (
            "GET /json/list HTTP/1.1\r\n" +
                "Host: 127.0.0.1\r\n" +
                "X-SGT-Bridge-Token: $token\r\n" +
                "\r\n"
            ).toByteArray(StandardCharsets.ISO_8859_1)
}
