package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChromeDevToolsTargetTest {
    @Test
    fun parserKeepsOnlyUniqueExactPageTargets() {
        val parsed = parseChromeTargets(
            Json.parseToJsonElement(
                """
                [
                  {
                    "id": "page_A-1",
                    "type": "page",
                    "title": "Current page",
                    "url": "https://example.com/current",
                    "webSocketDebuggerUrl":
                      "ws://127.0.0.1/devtools/page/page_A-1"
                  },
                  {
                    "id": "worker_1",
                    "type": "service_worker",
                    "title": "worker",
                    "url": "https://example.com/worker.js",
                    "webSocketDebuggerUrl":
                      "ws://127.0.0.1/devtools/page/worker_1"
                  },
                  {
                    "id": "page_A-1",
                    "type": "page",
                    "title": "duplicate",
                    "url": "https://example.com/duplicate",
                    "webSocketDebuggerUrl":
                      "ws://127.0.0.1/devtools/page/page_A-1"
                  }
                ]
                """.trimIndent(),
            ),
        )

        assertEquals(1, parsed.size)
        assertEquals("page_A-1", parsed.single().targetId)
        assertEquals("/devtools/page/page_A-1", parsed.single().webSocketPath)
    }

    @Test
    fun malformedIdentityAndAmbiguousSocketUrlsAreRejected() {
        assertTrue(validChromeTargetId("page_2-A"))
        assertTrue(!validChromeTargetId("../page"))
        assertNull(chromeWebSocketPath("http://127.0.0.1/devtools/page/1"))
        assertNull(chromeWebSocketPath("ws://127.0.0.1/not-devtools/page/1"))
        assertNull(chromeWebSocketPath("ws://127.0.0.1/devtools/page/1?token=secret"))
        assertEquals(
            "/devtools/page/1",
            chromeWebSocketPath("ws://127.0.0.1/devtools/page/1"),
        )
    }

    @Test
    fun launchDiscoveryRequiresOneChangedExactUrlTarget() {
        val requested = browserHttpUri("https://example.com/current")!!
        val before = ChromeTargetBaseline(
            mapOf(
                "unchanged" to "https://example.com/current",
                "reused" to "https://example.com/old",
            ),
        )
        val resolved = resolveLaunchedChromeTarget(
            requested,
            before,
            listOf(
                target("unchanged", "https://example.com/current"),
                target("reused", "https://example.com/current"),
            ),
        )

        assertEquals(
            "reused",
            (resolved as LaunchedChromeTargetResolution.Exact).target.targetId,
        )
    }

    @Test
    fun launchDiscoveryNormalizesDefaultPortAndRootPath() {
        val resolved = resolveLaunchedChromeTarget(
            browserHttpUri("https://example.com")!!,
            ChromeTargetBaseline(emptyMap()),
            listOf(target("new", "https://EXAMPLE.com:443/")),
        )

        assertEquals(
            "new",
            (resolved as LaunchedChromeTargetResolution.Exact).target.targetId,
        )
    }

    @Test
    fun launchDiscoveryRejectsConcurrentMatchingTargets() {
        val requested = browserHttpUri("https://example.com/current")!!
        val resolved = resolveLaunchedChromeTarget(
            requested,
            ChromeTargetBaseline(emptyMap()),
            listOf(
                target("first", requested.toString()),
                target("second", requested.toString()),
            ),
        )

        assertEquals(LaunchedChromeTargetResolution.Ambiguous, resolved)
    }

    private fun target(id: String, url: String) = ChromeDevToolsTarget(
        targetId = id,
        title = id,
        url = url,
        webSocketPath = "/devtools/page/$id",
    )
}
