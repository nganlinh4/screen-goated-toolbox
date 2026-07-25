package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.HttpUrl.Companion.toHttpUrl
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PublicWebResearchRequestTest {
    @Test
    fun validDomainRestrictedRequestMatchesOnlyTheDeclaredHostFamily() {
        val parsed = parseResearchRequest(buildJsonObject {
            put("query", "published limits")
            put("purpose", "current documented request limits")
            put("source_policy", "domain_restricted")
            put("allowed_domains", buildJsonArray { add(JsonPrimitive("example.com")) })
            put("max_sources", 3)
        }) as ResearchRequestResult.Ready

        assertEquals(SourcePolicy.DOMAIN_RESTRICTED, parsed.request.policy)
        assertEquals(3, parsed.request.maxSources)
        assertTrue(parsed.request.accepts("https://docs.example.com/limits".asHttpUrl()))
        assertFalse(parsed.request.accepts("https://notexample.com/limits".asHttpUrl()))
    }

    @Test
    fun privateLiteralAndCredentialedSourcesAreRejectedBeforeNetworkUse() {
        listOf(
            "http://127.0.0.1/private",
            "http://10.4.3.2/private",
            "https://name:secret@example.com/private",
            "http://[::1]/private",
        ).forEach { source ->
            val parsed = parseResearchRequest(buildJsonObject {
                put("query", "facts")
                put("purpose", "specific public facts")
                put("source_urls", buildJsonArray { add(JsonPrimitive(source)) })
            })
            assertTrue(source, parsed is ResearchRequestResult.Failure)
        }
    }

    @Test
    fun privateDomainRestrictionIsRejectedAndOrdinaryHostnameRemainsValid() {
        val privateDomain = parseResearchRequest(buildJsonObject {
            put("query", "facts")
            put("purpose", "specific public facts")
            put("source_policy", "domain_restricted")
            put("allowed_domains", buildJsonArray { add(JsonPrimitive("192.168.1.5")) })
        })
        val publicDomain = parseResearchRequest(buildJsonObject {
            put("query", "facts")
            put("purpose", "specific public facts")
            put("source_policy", "domain_restricted")
            put("allowed_domains", buildJsonArray { add(JsonPrimitive("docs.example.com")) })
        })

        assertTrue(privateDomain is ResearchRequestResult.Failure)
        assertTrue(publicDomain is ResearchRequestResult.Ready)
    }

    @Test
    fun networkPolicyRejectsReservedAddressesWithoutRejectingPublicNames() {
        assertFalse(publicNetworkHost("localhost"))
        assertFalse(publicNetworkHost("169.254.1.2"))
        assertFalse(publicNetworkHost("100.64.0.1"))
        assertFalse(publicNetworkHost("fc00::1"))
        assertTrue(publicNetworkHost("example.com"))
        assertTrue(publicNetworkHost("8.8.8.8"))
    }

    private fun String.asHttpUrl() = toHttpUrl()
}
