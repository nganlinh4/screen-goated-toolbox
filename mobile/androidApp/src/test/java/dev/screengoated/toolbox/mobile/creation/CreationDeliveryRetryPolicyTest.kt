package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.creation.CreationDeliveryTestFixture.prepared
import dev.screengoated.toolbox.mobile.creation.CreationDeliveryTestFixture.record
import dev.screengoated.toolbox.mobile.creation.CreationDeliveryTestFixture.request
import java.io.FileNotFoundException
import java.io.IOException
import kotlinx.serialization.encodeToString
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationDeliveryRetryPolicyTest {
    private val unpublished = record(prepared(request()))

    @Test
    fun transientFailureRetriesUntilTheAttemptBudget() {
        val error = IOException("No space left on device")
        (1 until CREATION_DELIVERY_MAXIMUM_PUBLISH_ATTEMPTS).forEach { attempts ->
            assertEquals(
                CreationDeliveryRetryDecision.RETRY,
                creationDeliveryRetryDecision(unpublished, attempts, error),
            )
        }
        assertEquals(
            CreationDeliveryRetryDecision.ABANDON,
            creationDeliveryRetryDecision(
                unpublished,
                CREATION_DELIVERY_MAXIMUM_PUBLISH_ATTEMPTS,
                error,
            ),
        )
    }

    @Test
    fun revokedGrantOrMissingDestinationAbandonsAtOnce() {
        listOf(
            SecurityException("Permission Denial"),
            FileNotFoundException("No such document"),
            IllegalStateException("wrapped", SecurityException("revoked")),
        ).forEach { error ->
            assertTrue(creationDeliveryErrorIsPermanent(error))
            assertEquals(
                CreationDeliveryRetryDecision.ABANDON,
                creationDeliveryRetryDecision(unpublished, 1, error),
            )
        }
        assertFalse(creationDeliveryErrorIsPermanent(IOException("busy")))
    }

    @Test
    fun aPublishedResultIsNeverAbandoned() {
        val published = unpublished.copy(
            pendingHandle = "content://media/external/downloads/1",
            pendingIdentity = "content://media/external/downloads/1",
            publicationPrepared = true,
            publishedPath = "content://media/external/downloads/1",
            transactionStage = "published",
        )
        assertEquals(
            CreationDeliveryRetryDecision.RETRY,
            creationDeliveryRetryDecision(
                published,
                CREATION_DELIVERY_MAXIMUM_PUBLISH_ATTEMPTS,
                SecurityException("revoked"),
            ),
        )
    }

    @Test
    fun failureHookCanSettleOneDeliveryWithoutStoppingOthers() {
        val first = record(prepared(request("3d_job_0", "dispatch_3d_job_0")))
        val second = record(prepared(request("3d_job_1", "dispatch_3d_job_1")))
        val failures = mutableListOf<String>()
        val settled = reconcileCreationDeliveriesIndependently(
            listOf(first, second),
            onFailure = { saved, _ ->
                failures += saved.request.jobId
                CreationReconciledTerminal(saved.request, creationFailedStatus(saved.current), null)
            },
        ) { saved ->
            if (saved === first) throw SecurityException("revoked")
            CreationReconciledTerminal(saved.request, saved.current.copy(stage = "done"), null)
        }
        assertEquals(listOf("3d_job_0"), failures)
        assertEquals("failed", settled.getValue("3d_job_0").status.stage)
        assertEquals(publicCreationFailure(), settled.getValue("3d_job_0").status.error)
        assertEquals("done", settled.getValue("3d_job_1").status.stage)
    }

    @Test
    fun aThrowingFailureHookKeepsTheDeliveryForTheNextRound() {
        val saved = record(prepared(request()))
        val settled = reconcileCreationDeliveriesIndependently(
            listOf(saved),
            onFailure = { _, _ -> throw IOException("state unavailable") },
        ) { throw IOException("destination unavailable") }
        assertTrue(settled.isEmpty())
    }

    @Test
    fun attemptCountSurvivesTheDurableRecordAndLegacyRecordsDefaultToZero() {
        val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
        val counted = unpublished.copy(publishAttempts = 3)
        val restored = json.decodeFromString<CreationDeliveryRecord>(json.encodeToString(counted))
        assertEquals(3, restored.publishAttempts)
        val legacy = json.encodeToString(unpublished).replace(",\"publishAttempts\":0", "")
        assertFalse(legacy.contains("publishAttempts"))
        assertEquals(0, json.decodeFromString<CreationDeliveryRecord>(legacy).publishAttempts)
    }
}
