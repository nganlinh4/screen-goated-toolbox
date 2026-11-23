package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.creation.CreationDeliveryTestFixture.prepared
import dev.screengoated.toolbox.mobile.creation.CreationDeliveryTestFixture.record
import dev.screengoated.toolbox.mobile.creation.CreationDeliveryTestFixture.request
import java.io.File
import java.io.IOException
import kotlin.io.path.createTempDirectory
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Test

class CreationDestinationAvailabilityTest {
    @Test
    fun deliveryBudgetsSurviveAnAbsentAcceptedRequest() {
        fixture().getValue("deliveryWithoutIntentCases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val root = createTempDirectory("creation-retained-delivery").toFile()
            try {
                val managed = case.getValue("managed").jsonPrimitive.boolean
                val destination = if (managed) "managed" else "content://storage/tree/unavailable"
                val saved = record(prepared(request("3d_job_0", "dispatch_3d_job_0"))).copy(
                    artifactSize = case.getValue("artifactBytes").jsonPrimitive.long,
                    publicationPrepared = case.getValue("prepared").jsonPrimitive.boolean,
                    intent = CreationPublishIntent(
                        kind = if (managed) "managed" else "saf", destination = destination,
                        finalName = "result.glb", mimeType = "model/gltf-binary",
                        pendingName = ".sgt-${"0".repeat(32)}.pending", reservationToken = "0".repeat(32),
                    ),
                    pendingHandle = "content://storage/document/unavailable",
                )
                val state = File(root, "creation/state").apply { mkdirs() }
                File(state, "deliveries.json").writeText(Json.encodeToString(listOf(saved)))
                val pending = creationPendingStorageReservations(root) { -1L }
                assertEquals(case.getValue("expectedManagedBytes").jsonPrimitive.long, pending.internalBytes)
                assertEquals(
                    if (managed) emptyMap<String, Long>() else mapOf(destination to case.getValue("expectedOutputReservationBytes").jsonPrimitive.long),
                    pending.destinationBytes,
                )
            } finally {
                root.deleteRecursively()
            }
        }
    }

    @Test
    fun unknownMaterializationKeepsEveryRemainingAllocation() {
        fixture().getValue("remainingOutputCases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val materialized = case.getValue("materializedBytes")
            var queries = 0
            val remaining = creationMissingDeliveryBytes(
                artifactBytes = case.getValue("artifactBytes").jsonPrimitive.long,
                pendingHandle = "content://storage/document/pending",
                publicationPrepared = case.getValue("prepared").jsonPrimitive.boolean,
            ) {
                queries += 1
                if (materialized == JsonNull) -1L else materialized.jsonPrimitive.long
            }
            assertEquals(case.toString(), case.getValue("expectedRemaining").jsonPrimitive.long, remaining)
            assertEquals(if (case.getValue("prepared").jsonPrimitive.boolean) 0 else 1, queries)
        }
    }

    @Test
    fun oneUnavailableHandoffDoesNotStopAnotherInEitherOrder() {
        fixture().getValue("reconciliationCases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val available = case.getValue("available").jsonArray.map { it.jsonPrimitive.boolean }
            val records = available.indices.map { index ->
                record(prepared(request("3d_job_$index", "dispatch_3d_job_$index")))
            }
            val before = Json.encodeToString(records)
            val completed = reconcileCreationDeliveriesIndependently(records) { saved ->
                val index = records.indexOf(saved)
                if (!available[index]) throw IOException("Destination is unavailable")
                CreationReconciledTerminal(saved.request, saved.current.copy(stage = "done"), null)
            }
            val completedIndices = records.indices.filter { records[it].request.jobId in completed }
            val retainedIndices = records.indices.filter { records[it].request.jobId !in completed }
            assertEquals(case.getValue("expectedCompleted").jsonArray.map { it.jsonPrimitive.int }, completedIndices)
            assertEquals(case.getValue("expectedRetained").jsonArray.map { it.jsonPrimitive.int }, retainedIndices)
            assertEquals(before, Json.encodeToString(records))
        }
    }

    @Test
    fun pendingDestinationsRetainTheirIndependentBudgetsWhenOneIsUnobservable() {
        val root = createTempDirectory("creation-destination-reservations").toFile()
        try {
            val state = File(root, "creation/state").apply { mkdirs() }
            val destinations = listOf("content://storage/tree/unavailable", "content://storage/tree/available")
            val records = destinations.mapIndexed { index, destination ->
                val token = index.toString().repeat(32)
                record(prepared(request("3d_job_$index", "dispatch_3d_job_$index"))).copy(
                    artifactSize = 1_024,
                    intent = CreationPublishIntent(
                        kind = "saf", destination = destination, finalName = "result.glb",
                        mimeType = "model/gltf-binary", pendingName = ".sgt-$token.pending",
                        reservationToken = token,
                    ),
                    pendingHandle = "content://storage/document/pending-$index",
                )
            }
            File(state, "deliveries.json").writeText(Json.encodeToString(records))
            val before = File(state, "deliveries.json").readBytes()
            val pending = creationPendingStorageReservations(root) { path ->
                if (path.endsWith("pending-0")) -1L else 0L
            }
            assertEquals(0L, pending.internalBytes)
            assertEquals(destinations.associateWith { 1_024L }, pending.destinationBytes)
            assertEquals(before.toList(), File(state, "deliveries.json").readBytes().toList())
        } finally {
            root.deleteRecursively()
        }
    }

    private fun fixture() = Json.parseToJsonElement(fixtureFile().readText()).jsonObject

    private fun fixtureFile(): File {
        var directory: File? = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (directory != null) {
            val candidate = File(directory, "parity-fixtures/creation/destination-availability.json")
            if (candidate.isFile) return candidate
            directory = directory.parentFile
        }
        error("Creation destination availability fixture not found")
    }
}
