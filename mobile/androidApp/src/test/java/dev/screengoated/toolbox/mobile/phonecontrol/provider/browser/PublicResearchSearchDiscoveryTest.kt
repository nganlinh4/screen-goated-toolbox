package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import okhttp3.HttpUrl.Companion.toHttpUrl
import org.jsoup.Jsoup
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PublicResearchSearchDiscoveryTest {
    private val provider = SearchProvider(
        endpoint = "https://search.invalid/find",
        transportHost = "search.invalid",
        redirectPath = "/out",
        redirectParameters = setOf("target"),
    )

    @Test
    fun interstitialWithOnlyOffHostPolicyLinksYieldsNoResults() {
        val page = document(
            """
            <h1>Before you continue</h1>
            <form action="/save"><p>Read our <a href="https://policies.example/privacy">privacy policy</a>
            and <a href="https://help.example/cookies">cookie help</a>.</p><button>Accept</button></form>
            <footer><a href="https://apps.example/download">Get the app</a></footer>
            """,
        )

        assertTrue(provider.resultLinks(page).isEmpty())
    }

    @Test
    fun onlyStructuralVisibleLabelledResultLinksBecomeCandidates() {
        val page = document(
            """
            <header><a href="https://chrome.example/account">Account</a></header>
            <nav><a href="https://chrome.example/images">Images</a></nav>
            <main>
              <div><a href="https://first.example/page"><h3>First result</h3></a></div>
              <div><h3><a href="/out?target=https%3A%2F%2Fsecond.example%2Fdoc">Second result</a></h3></div>
              <div aria-hidden="true"><h3><a href="https://hidden.example/">Hidden result</a></h3></div>
              <div style="display: none"><h3><a href="https://styled.example/">Styled away</a></h3></div>
              <div><a href="https://unlabelled.example/"></a></div>
              <div><a href="/settings">Search settings</a></div>
            </main>
            <p><a href="https://loose.example/">Loose paragraph link</a></p>
            <footer><a href="https://chrome.example/privacy">Privacy</a></footer>
            """,
        )

        assertEquals(
            listOf(
                "https://first.example/page" to "First result",
                "https://second.example/doc" to "Second result",
            ),
            provider.resultLinks(page).map { it.url.toString() to it.label },
        )
    }

    @Test
    fun onlyTheResultEndpointIsAResultPage() {
        assertTrue(provider.isResultPage(provider.searchUrl("two words")))
        assertTrue(provider.isResultPage("https://SEARCH.invalid/find?q=x".toHttpUrl()))
        assertFalse(provider.isResultPage("https://consent.search.invalid/find?q=x".toHttpUrl()))
        assertFalse(provider.isResultPage("https://search.invalid/sorry/index?q=x".toHttpUrl()))
    }

    private fun document(body: String) =
        Jsoup.parse("<html><head><title>t</title></head><body>$body</body></html>", "https://search.invalid/find?q=x")
}
