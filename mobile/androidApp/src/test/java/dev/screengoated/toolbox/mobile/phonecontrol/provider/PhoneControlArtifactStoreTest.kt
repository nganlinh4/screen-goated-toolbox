package dev.screengoated.toolbox.mobile.phonecontrol.provider

import java.io.File
import java.io.InputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class PhoneControlArtifactStoreTest {
    private fun store(bytes: Int = PhoneControlArtifactStore.MAXIMUM_ARTIFACT_BYTES, entries: Int = PhoneControlArtifactStore.MAXIMUM_ARTIFACT_ENTRIES) =
        PhoneControlArtifactStore(bytes, entries) { 0L }

    @Test fun sharedLifecycleCasesAndLimits() {
        val fixture = fixture()
        val limits = fixture.getValue("limits").jsonObject
        assertEquals(limits.getValue("maxRetainedBytes").jsonPrimitive.int, PhoneControlArtifactStore.MAXIMUM_ARTIFACT_BYTES)
        assertEquals(limits.getValue("maxEntries").jsonPrimitive.int, PhoneControlArtifactStore.MAXIMUM_ARTIFACT_ENTRIES)
        fixture.getValue("cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            when (case.getValue("operation").jsonPrimitive.content) {
                "quota" -> store(entries = case.getValue("maxEntries").jsonPrimitive.int).use { artifacts ->
                    val values = case.getValue("values").jsonArray
                    val first = artifacts.put(values[0].jsonPrimitive.content.toByteArray(), "text/plain")
                    expectRejection(ArtifactStoreRejection.LIMIT) { artifacts.put(values[1].jsonPrimitive.content.toByteArray(), "text/plain") }
                    assertEquals(case.getValue("firstValueAfterFailure").jsonPrimitive.content, artifacts.get(first.id)!!.bytes.toString(Charsets.UTF_8))
                }
                "retire" -> {
                    val artifacts = store()
                    val artifact = artifacts.put(case.getValue("value").jsonPrimitive.content.toByteArray(), "text/plain")
                    artifacts.close()
                    artifacts.close()
                    assertNull(artifacts.get(artifact.id))
                    expectRejection(ArtifactStoreRejection.RETIRED) { artifacts.put(byteArrayOf(1), "text/plain") }
                }
                "replace_session" -> {
                    val first = store()
                    val bytes = case.getValue("value").jsonPrimitive.content.toByteArray()
                    val oldId = first.put(bytes, "text/plain").id
                    first.close()
                    store().use { next ->
                        assertNull(next.get(oldId))
                        assertNotEquals(oldId, next.put(bytes, "text/plain").id)
                    }
                }
                else -> error("Unsupported artifact fixture operation")
            }
        }
    }

    @Test fun payloadAndDigestAreIsolatedFromCallerMutation() = store().use { artifacts ->
        val input = byteArrayOf(1, 2, 3)
        val artifact = artifacts.put(input, "application/octet-stream")
        val digest = artifact.sha256
        input[0] = 9
        artifact.bytes[1] = 9
        artifacts.get(artifact.id)!!.bytes[2] = 9
        assertArrayEquals(byteArrayOf(1, 2, 3), artifact.bytes)
        assertEquals(digest, artifacts.get(artifact.id)!!.sha256)
    }

    @Test fun utf8MetadataSharesThePayloadBudgetAndInvalidMetadataDoesNotPublish() = store(bytes = 8).use { artifacts ->
        expectRejection(ArtifactStoreRejection.INVALID_METADATA) { artifacts.put(byteArrayOf(), "x", "\uD800") }
        val first = artifacts.put(byteArrayOf(1, 2), "x", "\uD83D\uDE00")
        expectRejection(ArtifactStoreRejection.LIMIT) { artifacts.put(byteArrayOf(3), "x") }
        assertArrayEquals(byteArrayOf(1, 2), artifacts.get(first.id)!!.bytes)
    }

    @Test fun concurrentAdmissionCannotExceedTheEntryBudget() {
        val executor = Executors.newFixedThreadPool(8)
        val start = CountDownLatch(1)
        store(entries = 4).use { artifacts ->
            try {
                val jobs = (0 until 16).map {
                    executor.submit<String?> {
                        start.await()
                        try { artifacts.put(byteArrayOf(1), "x").id }
                        catch (error: PhoneControlArtifactStoreException) {
                            assertEquals(ArtifactStoreRejection.LIMIT, error.rejection)
                            null
                        }
                    }
                }
                start.countDown()
                val accepted = jobs.mapNotNull { it.get(5, TimeUnit.SECONDS) }
                assertEquals(4, accepted.size)
                accepted.forEach { assertNotNull(artifacts.get(it)) }
            } finally { executor.shutdownNow() }
        }
    }

    @Test fun lateWorkerCannotPublishAfterRetirement() {
        val artifacts = store()
        val executor = Executors.newSingleThreadExecutor()
        val proceed = CountDownLatch(1)
        try {
            val late = executor.submit {
                proceed.await()
                expectRejection(ArtifactStoreRejection.RETIRED) { artifacts.put(byteArrayOf(1), "x") }
            }
            artifacts.close()
            proceed.countDown()
            late.get(5, TimeUnit.SECONDS)
        } finally { executor.shutdownNow(); artifacts.close() }
    }

    @Test fun retirementDuringClockSamplingCannotReopenTheStore() {
        lateinit var artifacts: PhoneControlArtifactStore
        artifacts = PhoneControlArtifactStore(nowMillis = { artifacts.close(); 0L })
        expectRejection(ArtifactStoreRejection.RETIRED) { artifacts.put(byteArrayOf(1), "x") }
    }

    @Test fun endlessInputStopsAtTheBoundWithoutAllocatingTheWholeStream() {
        var read = 0
        val input = object : InputStream() {
            override fun read(): Int { read++; return 1 }
        }
        try { readArtifactBytesBounded(input, 64); fail("Oversized input accepted") }
        catch (_: IllegalArgumentException) { assertEquals(65, read) }
    }

    private fun expectRejection(reason: ArtifactStoreRejection, block: () -> Unit) {
        try { block(); fail("Expected an artifact rejection") }
        catch (error: PhoneControlArtifactStoreException) { assertEquals(reason, error.rejection) }
    }

    private fun fixture(): JsonObject {
        val path = "parity-fixtures/phone-control/artifact-lifecycle.json"
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .first { File(it, path).isFile }
        return Json.parseToJsonElement(File(root, path).readText()).jsonObject
    }
}
