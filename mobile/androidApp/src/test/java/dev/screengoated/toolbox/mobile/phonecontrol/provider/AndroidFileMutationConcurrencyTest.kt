package dev.screengoated.toolbox.mobile.phonecontrol.provider

import java.io.File
import java.nio.file.Files
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import java.util.concurrent.CountDownLatch
import java.util.concurrent.CyclicBarrier
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class AndroidFileMutationConcurrencyTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val executors = mutableListOf<java.util.concurrent.ExecutorService>()

    @After
    fun stopExecutors() {
        executors.forEach { it.shutdownNow() }
    }

    @Test
    fun `concurrent create-only saves never overwrite the winner`() {
        val first = artifact("first", "first bytes")
        val second = artifact("second", "second bytes")
        val artifacts = mapOf(first.id to first, second.id to second)
        val provider = testFileProvider(artifacts::get)
        val executor = newExecutor()

        repeat(RACE_ITERATIONS) { iteration ->
            val destination = File(temporaryFolder.root, "shared-$iteration.bin")
            val barrier = CyclicBarrier(3)
            val futures = listOf(first.id, second.id).map { id ->
                executor.submit<AndroidProviderResult> {
                    provider.prepareArtifactSave(id, destination.absolutePath, overwrite = false).use { prepared ->
                        barrier.await(5, TimeUnit.SECONDS)
                        prepared.commit(testFileOwner(), { true })
                    }
                }
            }
            barrier.await(5, TimeUnit.SECONDS)
            val results = futures.map { it.get(5, TimeUnit.SECONDS) }

            assertEquals(1, results.count { it is AndroidProviderResult.Success })
            val rejected = results.single { it is AndroidProviderResult.Failure }
                as AndroidProviderResult.Failure
            assertEquals(
                expected("concurrent create preserves one complete winner", "rejectedCode"),
                rejected.code,
            )
            assertTrue(
                destination.readBytes().contentEquals(first.bytes) ||
                    destination.readBytes().contentEquals(second.bytes),
            )
        }
    }

    @Test
    fun `same snapshot exact replacements are serialized`() {
        val provider = testFileProvider { null }
        val executor = newExecutor()

        repeat(RACE_ITERATIONS) { iteration ->
            val destination = temporaryFolder.newFile("document-$iteration.txt").apply {
                writeText("base")
            }
            val expectedSha256 = destination.readBytes().sha256()
            val barrier = CyclicBarrier(3)
            val futures = listOf("first", "second").map { replacement ->
                executor.submit<AndroidProviderResult> {
                    provider.prepareTarget(destination.absolutePath).use { target ->
                        barrier.await(5, TimeUnit.SECONDS)
                        provider.exactReplace(target, expectedSha256, listOf(ExactReplacement("base", replacement, 1)), testFileOwner(), { true })
                    }
                }
            }
            barrier.await(5, TimeUnit.SECONDS)
            val results = futures.map { it.get(5, TimeUnit.SECONDS) }

            assertEquals(1, results.count { it is AndroidProviderResult.Success })
            val rejected = results.single { it is AndroidProviderResult.Failure }
                as AndroidProviderResult.Failure
            assertEquals("hash_mismatch", rejected.code)
            assertTrue(destination.readText() in setOf("first", "second"))
        }
    }

    @Test
    fun `authorized missing target cannot overwrite a newly appeared file`() {
        val artifact = artifact("output", "authorized bytes")
        val provider = testFileProvider(mapOf(artifact.id to artifact)::get)
        val destination = File(temporaryFolder.root, "appeared.txt")
        val prepared = provider.prepareArtifactSave(artifact.id, destination.absolutePath, overwrite = true)
        destination.writeText("competing bytes")
        val result = prepared.use { it.commit(testFileOwner(), { true }) }

        val failure = assertFailure(
            result,
            expected(
                "authorized missing target cannot overwrite a newly appeared file",
                "resultCode",
            ),
        )
        assertTrue(failure.data.getValue("original_unchanged").jsonPrimitive.boolean)
        assertEquals("competing bytes", destination.readText())
    }

    @Test
    fun `authorized canonical target cannot drift before exact commit`() {
        val parent = temporaryFolder.newFolder("approved-parent").canonicalFile
        val destination = File(parent, "document.txt").apply { writeText("base") }
        val moved = File(temporaryFolder.root, "moved-parent")
        val io = NioTestFileTransactionIo()
        val provider = AndroidFileProvider(findArtifact = { null }, fileIo = io)
        val result = provider.prepareTarget(destination.absolutePath).use { target ->
            Files.move(parent.toPath(), moved.toPath())
            check(parent.mkdir())
            io.replaceIdentity(parent.absolutePath)
            destination.writeText("base")
            provider.exactReplace(target, "base".toByteArray().sha256(),
                listOf(ExactReplacement("base", "changed", 1)), testFileOwner(), { true })
        }
        assertFailure(result, expected("authorized canonical target cannot drift before exact commit", "resultCode"))
        assertEquals("base", destination.readText())
        assertEquals("base", File(moved, "document.txt").readText())
    }

    @Test
    fun `external modification after staging is rejected at commit`() {
        val destination = temporaryFolder.newFile("document.txt").apply { writeText("base") }
        val stagedReady = CountDownLatch(1)
        val externalWriteFinished = CountDownLatch(1)
        val io = NioTestFileTransactionIo().apply {
            beforeWrite = { stagedReady.countDown(); check(externalWriteFinished.await(5, TimeUnit.SECONDS)) }
        }
        val provider = AndroidFileProvider(findArtifact = { null }, fileIo = io)
        val target = provider.prepareTarget(destination.absolutePath)
        val future = newExecutor().submit<AndroidProviderResult> {
            target.use { provider.exactReplace(it, "base".toByteArray().sha256(),
                listOf(ExactReplacement("base", "tool bytes", 1)), testFileOwner(), { true }) }
        }
        assertTrue(stagedReady.await(5, TimeUnit.SECONDS))
        destination.writeText("external bytes")
        externalWriteFinished.countDown()
        val result = future.get(5, TimeUnit.SECONDS)
        val failure = assertFailure(result, expected("concurrent modifier with retained staging is preserved before exact commit", "resultCode"))
        assertEquals(dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty.MAY_HAVE_OCCURRED, failure.effect)
        assertEquals(false, failure.data.getValue("target_effect_may_have_occurred").jsonPrimitive.boolean)
        assertTrue(failure.data.containsKey("retained_staging_path"))
        assertEquals("external bytes", destination.readText())
        assertFalse(destination.readText() == "tool bytes")
    }

    private fun artifact(id: String, text: String) = PhoneControlArtifact(
        id = id,
        bytes = text.toByteArray(),
        mimeType = "application/octet-stream",
        name = null,
        createdAtMs = 0L,
    )

    private fun newExecutor() = Executors.newFixedThreadPool(2).also(executors::add)

    private fun assertFailure(
        result: AndroidProviderResult,
        code: String,
    ): AndroidProviderResult.Failure {
        assertTrue(result is AndroidProviderResult.Failure)
        return (result as AndroidProviderResult.Failure).also {
            assertEquals(code, it.code)
        }
    }

    private fun expected(caseName: String, field: String): String = contract
        .getValue("cases")
        .jsonArray
        .map { it.jsonObject }
        .single { it.getValue("name").jsonPrimitive.content == caseName }
        .getValue("expect")
        .jsonObject
        .getValue(field)
        .jsonPrimitive
        .content

    private val contract by lazy {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val root = generateSequence(File(workingDirectory).canonicalFile) { it.parentFile }
            .first { File(it, CONTRACT_PATH).isFile }
        Json.parseToJsonElement(File(root, CONTRACT_PATH).readText()).jsonObject
    }
}

private const val CONTRACT_PATH = "parity-fixtures/phone-control/file-mutation-contract.json"
private const val RACE_ITERATIONS = 32
