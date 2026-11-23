package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.io.IOException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class CreationCancellationStoreTest {
    @get:Rule val temporary = TemporaryFolder()

    private fun request(): CreationWorkerRequest {
        val unsigned = CreationWorkerRequest(
            jobId = "3d_job", dispatchId = "dispatch_3d_job",
            sourceDescriptors = listOf(
                CreationSourceDescriptor("creation/job-inputs/source.png", 4, "b".repeat(64)),
            ),
            tool = CreationTool.IMAGE_TO_3D.wireName,
            generationMode = CreationGenerationMode.QUALITY.wireName,
            operation = "generate", imagePath = "creation/job-inputs/source.png",
            outputPath = "creation/staging/result.glb", outputName = "result.glb",
        )
        return unsigned.copy(requestFingerprint = creationRequestFingerprint(unsigned))
    }

    @Test
    fun `unavailable cancellation leaves the dispatch active`() {
        val store = CreationCancellationStore(temporary.newFolder()) { _, _, _ ->
            throw IOException("storage unavailable")
        }
        val request = request()
        assertTrue(runCatching { store.record(listOf(request)) }.exceptionOrNull() is IOException)
        assertFalse(store.isCancelled(request))
        assertEquals("delivered", store.ifActive(request) { "delivered" })
    }

    @Test
    fun `committed cancellation blocks a late completion with the same identity`() {
        val directory = temporary.newFolder()
        val store = CreationCancellationStore(directory) { target, text, _ -> target.writeText(text) }
        val request = request()
        store.record(listOf(request))
        assertTrue(store.isCancelled(request))
        var published = false
        assertTrue(runCatching { store.ifActive(request) { published = true } }.isFailure)
        assertFalse(published)
        assertTrue(File(directory, "cancellations.json").isFile)
    }

    @Test
    fun `pending cancellation does not discard completion when persistence fails`() {
        val writing = CountDownLatch(1)
        val release = CountDownLatch(1)
        val completionEntered = CountDownLatch(1)
        val executor = Executors.newFixedThreadPool(2)
        val store = CreationCancellationStore(temporary.newFolder()) { _, _, _ ->
            writing.countDown()
            check(release.await(5, TimeUnit.SECONDS))
            throw IOException("storage unavailable")
        }
        val request = request()
        try {
            val cancellation = executor.submit<Throwable?> {
                runCatching { store.record(listOf(request)) }.exceptionOrNull()
            }
            assertTrue(writing.await(5, TimeUnit.SECONDS))
            val completion = executor.submit<String> {
                completionEntered.countDown()
                store.ifActive(request) { "delivered" }
            }
            assertTrue(completionEntered.await(5, TimeUnit.SECONDS))
            assertTrue(runCatching { completion.get(50, TimeUnit.MILLISECONDS) }.exceptionOrNull() is TimeoutException)
            release.countDown()
            assertTrue(cancellation.get(5, TimeUnit.SECONDS) is IOException)
            assertEquals("delivered", completion.get(5, TimeUnit.SECONDS))
            assertFalse(store.isCancelled(request))
        } finally {
            release.countDown()
            executor.shutdownNow()
        }
    }
}
