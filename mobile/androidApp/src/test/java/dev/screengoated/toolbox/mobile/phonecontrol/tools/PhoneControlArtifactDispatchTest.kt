package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlAuthorityFixture
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouter
import dev.screengoated.toolbox.mobile.phonecontrol.provider.ArtifactStoreRejection
import dev.screengoated.toolbox.mobile.phonecontrol.provider.PhoneControlArtifactStoreException
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Test

class PhoneControlArtifactDispatchTest {
    @Test fun dispatchRetirementReachesTheOwnedHandler() {
        var closed = false
        val executor = object : PhoneControlHandlerExecutor {
            override fun close() { closed = true }
            override suspend fun execute(handler: PhoneControlHandler, job: PhoneControlToolJobContext, requestedTool: String, arguments: JsonObject): PhoneControlToolExecution = error("No dispatch expected")
        }
        dispatcher(executor).close()
        assertTrue(closed)
    }

    @Test fun artifactQuotaIsTypedAndReadOnlyFailureDoesNotClaimAnEffect() = runTest {
        val dispatcher = dispatcher(PhoneControlHandlerExecutor { _, _, _, _ ->
            throw PhoneControlArtifactStoreException(ArtifactStoreRejection.LIMIT)
        })
        val result = dispatcher.dispatch(PhoneControlToolJobContext(7, "artifact-job", 11), "artifact_info", JsonObject(emptyMap()))
        assertEquals("artifact_limit_reached", result.response.getValue("code").jsonPrimitive.content)
        assertEquals(EffectCertainty.PROVEN_NO_EFFECT.wireName, result.response.getValue("effect_status").jsonPrimitive.content)
        assertEquals("false", result.response.getValue("retryable").jsonPrimitive.content)
    }

    private fun dispatcher(executor: PhoneControlHandlerExecutor): PhoneControlToolDispatcher {
        val fixture = PhoneControlAuthorityFixture.load()
        return PhoneControlToolDispatcher(executor, ProviderRouter(fixture.providers, fixture.routes),
            failureReporter = PhoneControlToolFailureReporter { _, _, _ -> error("Unexpected generic failure") })
    }
}
