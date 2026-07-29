package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.io.ByteArrayInputStream
import java.io.InputStream
import java.nio.file.Files
import kotlin.io.path.createTempDirectory
import kotlin.concurrent.thread
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationFileStoreContractTest {
    @Test
    fun `unknown length stream stops at cap and removes partial file`() {
        val directory = createTempDirectory("creation-import-test").toFile()
        val target = File(directory, "partial.pending")
        val input = object : InputStream() {
            private var remaining = 12
            override fun read(): Int = if (remaining-- > 0) 1 else -1
        }

        assertThrows(IllegalArgumentException::class.java) {
            copyCreationInputBounded(input, target, maximumBytes = 8)
        }
        assertFalse(target.exists())
        directory.delete()
    }

    @Test
    fun `preview stream stops before an unknown provider can allocate past cap`() {
        val input = object : InputStream() {
            private var remaining = 12
            override fun read(): Int = if (remaining-- > 0) 1 else -1
        }

        assertThrows(IllegalArgumentException::class.java) {
            readCreationBytesBounded(input, maximumBytes = 8)
        }
    }

    @Test
    fun `managed cleanup never traverses a symbolic link`() {
        val root = createTempDirectory("creation-root").toFile()
        val outside = createTempDirectory("creation-outside").toFile()
        val outsideFile = File(outside, "keep.bin").apply { writeBytes(byteArrayOf(1, 2, 3)) }
        val link = File(root, "linked")
        val linked = runCatching {
            Files.createSymbolicLink(link.toPath(), outside.toPath())
            true
        }.getOrDefault(false)
        if (!linked) {
            root.delete()
            outsideFile.delete()
            outside.delete()
            return
        }

        assertTrue(creationRegularFilesNoFollow(root).isEmpty())
        assertFalse(deleteCreationFileConfined(root, File(link, outsideFile.name)))
        assertTrue(deleteCreationTreeNoFollow(root, link))
        assertFalse(link.exists())
        assertTrue(outsideFile.isFile)

        root.delete()
        outsideFile.delete()
        outside.delete()
    }

    @Test
    fun `published media and document outputs are never app managed`() {
        assertTrue(isUserOwnedCreationOutputPath("content://media/external/downloads/7"))
        assertTrue(
            isUserOwnedCreationOutputPath(
                "content://com.android.externalstorage.documents/document/primary%3ADownload",
            ),
        )
        assertFalse(isUserOwnedCreationOutputPath("/data/user/0/app/files/creation/result.png"))
    }

    @Test
    fun `artifact budget excludes journals history and published output`() {
        val files = File("app-files")
        val cache = File("app-cache")

        assertEquals(
            listOf(
                File(files, "creation/sources"),
                File(files, "creation/job-inputs"),
                File(files, "creation/library"),
                File(files, "creation/presentation"),
                File(files, "creation/relinquished"),
                File(files, "creation/staging"),
                File(cache, "creation/previews"),
            ),
            creationManagedArtifactRoots(files, cache),
        )
    }

    @Test
    fun `preview keys resist known Java hash collisions`() {
        val first = "content://preview/Aa"
        val second = "content://preview/BB"
        assertEquals(first.hashCode(), second.hashCode())

        assertTrue(
            creationPreviewCacheKey(first, "png", "4:1") !=
                creationPreviewCacheKey(second, "png", "4:1"),
        )
    }

    @Test
    fun `unversioned same-size preview source is recopied`() {
        val directory = createTempDirectory("creation-preview-mutation").toFile()
        val cache = CreationPreviewCache()
        val key = creationPreviewCacheKey("content://preview/item", "bin", null)
        var bytes = "AAAA".encodeToByteArray()

        cache.materialize(
            directory, key, "bin", 4, false,
            openInput = { ByteArrayInputStream(bytes) },
            validate = { require(it.length() == 4L) },
        )
        bytes = "BBBB".encodeToByteArray()
        val updated = cache.materialize(
            directory, key, "bin", 4, false,
            openInput = { ByteArrayInputStream(bytes) },
            validate = { require(it.length() == 4L) },
        )

        assertEquals("BBBB", updated.readText())
        updated.delete()
        directory.delete()
    }

    @Test
    fun `parallel preview materialization leaves one valid artifact and no temporary files`() {
        val directory = createTempDirectory("creation-preview-parallel").toFile()
        val cache = CreationPreviewCache()
        val key = creationPreviewCacheKey("content://preview/shared", "bin", null)
        val threads = (0 until 8).map {
            thread {
                cache.materialize(
                    directory, key, "bin", 8, false,
                    openInput = { ByteArrayInputStream("complete".encodeToByteArray()) },
                    validate = { require(it.readText() == "complete") },
                )
            }
        }
        threads.forEach(Thread::join)

        val files = directory.listFiles().orEmpty()
        assertEquals(1, files.size)
        assertEquals("complete", files.single().readText())
        assertFalse(files.any { ".tmp-" in it.name })
        files.single().delete()
        directory.delete()
    }

    @Test
    fun `history retention counts shared references once and evicts oldest first`() {
        val sizes = mapOf(
            "shared" to 60L,
            "old-only" to 30L,
            "middle-only" to 30L,
            "new-only" to 30L,
            "live" to 20L,
        )
        val kept = planCreationHistoryRetention(
            entries = listOf(
                retentionItem("old", 1, "shared", "old-only"),
                retentionItem("middle", 2, "shared", "middle-only"),
                retentionItem("new", 3, "new-only"),
            ),
            maximumPerTool = 10,
            budgetBytes = 150,
            protectedManagedPaths = setOf("live"),
            sizeOf = { sizes.getValue(it) },
        )

        assertEquals(setOf("middle", "new"), kept)
    }

    @Test
    fun `history retention preserves newest and live recovery paths under pressure`() {
        val sizes = mapOf("live" to 80L, "old" to 60L, "new" to 20L)
        val kept = planCreationHistoryRetention(
            entries = listOf(
                retentionItem("old", 1, "old"),
                retentionItem("new", 2, "new"),
            ),
            maximumPerTool = 10,
            budgetBytes = 100,
            protectedManagedPaths = setOf("live"),
            sizeOf = { sizes.getValue(it) },
        )

        assertEquals(setOf("new"), kept)
    }

    @Test
    fun `closing one surface preserves a shared source until the last surface releases`() {
        val leases = CreationSourceHandleLeases()
        leases.update("surface-a", setOf("shared", "only-a"))
        leases.update("surface-b", setOf("shared"))

        assertEquals(setOf("only-a"), leases.release("surface-a"))
        assertEquals(setOf("shared"), leases.release("surface-b"))
    }

    @Test
    fun `accepted managed inputs use independent hard link leases counted once`() {
        val filesDir = createTempDirectory("creation-hardlink-source").toFile()
        val source = File(filesDir, "creation/sources/source.img").apply {
            parentFile?.mkdirs()
            writeBytes(byteArrayOf(1, 2, 3, 4))
        }
        val first = File(filesDir, "creation/job-inputs/job-a/0.img").apply {
            parentFile?.mkdirs()
        }
        val second = File(filesDir, "creation/job-inputs/job-b/0.img").apply {
            parentFile?.mkdirs()
        }

        assertTrue(linkCreationAcceptedInput(filesDir, source.absolutePath, first))
        assertTrue(linkCreationAcceptedInput(filesDir, source.absolutePath, second))
        assertTrue(Files.isSameFile(source.toPath(), first.toPath()))
        val snapshot = snapshotCreationManagedStorage(
            listOf(
                File(filesDir, "creation/sources"),
                File(filesDir, "creation/sources"),
                File(filesDir, "creation/job-inputs"),
            ),
            setOf(first.absolutePath),
        )
        assertEquals(4L, snapshot.totalBytes)
        assertEquals(4L, snapshot.protectedBytes)

        assertTrue(source.delete())
        assertTrue(first.isFile)
        assertTrue(second.isFile)
        filesDir.deleteRecursively()
    }

    @Test
    fun `storage pressure prunes to recovery watermark with hysteresis`() {
        val gib = 1024L * 1024 * 1024
        val required = gib
        assertEquals(
            5 * gib / 2,
            creationPressurePruneBudget(
                totalManagedBytes = 3 * gib,
                availableBytes = required,
                requiredAvailableBytes = required,
                capBudgetBytes = 4 * gib,
            ),
        )
        assertEquals(
            4 * gib,
            creationPressurePruneBudget(
                totalManagedBytes = 3 * gib,
                availableBytes = required + CREATION_STORAGE_PRESSURE_TRIGGER_BYTES,
                requiredAvailableBytes = required,
                capBudgetBytes = 4 * gib,
            ),
        )
    }

    @Test
    fun `storage admission reserves source result and one gibibyte free`() {
        val gib = 1024L * 1024 * 1024
        val plan = planCreationStorageAdmission(
            totalManagedBytes = 2 * gib,
            protectedManagedBytes = gib,
            availableBytes = 2 * gib,
            additionalBytes = 512L * 1024 * 1024,
        )

        assertEquals(3_758_096_384L, plan.pruneBudgetBytes)
        assertEquals(1_610_612_736L, plan.requiredAvailableBytes)
        assertTrue(plan.accepted)
    }

    @Test
    fun `storage admission rejects protected pressure and insufficient free reserve`() {
        val gib = 1024L * 1024 * 1024

        assertFalse(
            planCreationStorageAdmission(
                totalManagedBytes = 4 * gib,
                protectedManagedBytes = 4 * gib,
                availableBytes = 8 * gib,
                additionalBytes = 1,
            ).accepted,
        )
        assertFalse(
            planCreationStorageAdmission(
                totalManagedBytes = gib,
                protectedManagedBytes = gib,
                availableBytes = gib,
                additionalBytes = 1,
            ).accepted,
        )
    }

    @Test
    fun `history pre-admission budget targets owned results without trusting unknown bytes`() {
        val gib = 1024L * 1024 * 1024
        assertEquals(
            3 * gib - 100,
            creationHistoryAdmissionBudget(
                totalManagedBytes = 4 * gib,
                historyOwnedBytes = 3 * gib,
                globalBudgetBytes = 4 * gib - 100,
            ),
        )
        assertEquals(
            0L,
            creationHistoryAdmissionBudget(
                totalManagedBytes = 4 * gib,
                historyOwnedBytes = gib,
                globalBudgetBytes = 2 * gib,
            ),
        )
    }

    @Test
    fun `creation index reads reject oversized files before decoding`() {
        val directory = createTempDirectory("creation-index-test").toFile()
        val index = File(directory, "history.json").apply { writeText("123456789") }

        assertEquals(null, readCreationIndexTextBounded(index, maximumBytes = 8))
        assertEquals("123456789", readCreationIndexTextBounded(index, maximumBytes = 9))

        index.delete()
        directory.delete()
    }

    @Test
    fun `oversized atomic index write preserves the prior readable state`() {
        val directory = createTempDirectory("creation-index-write-test").toFile()
        val index = File(directory, "history.json").apply { writeText("old") }

        assertThrows(IllegalArgumentException::class.java) {
            writeCreationIndexTextAtomically(index, "too-large", maximumBytes = 4)
        }
        assertEquals("old", index.readText())

        index.delete()
        directory.delete()
    }

    @Test
    fun `parallel index writes publish one complete value without shared temp files`() {
        val directory = createTempDirectory("creation-index-parallel").toFile()
        val index = File(directory, "history.json")
        val values = (0 until 12).map { indexValue -> "[$indexValue-${"x".repeat(2_048)}]" }
        val threads = values.map { value ->
            thread {
                repeat(10) {
                    writeCreationIndexTextAtomically(index, value, maximumBytes = 64 * 1024)
                }
            }
        }
        threads.forEach(Thread::join)

        assertTrue(index.readText() in values)
        assertFalse(directory.listFiles().orEmpty().any { ".tmp-" in it.name })

        index.delete()
        directory.delete()
    }

    @Test
    fun `corrupt durable state is not treated as an empty protection set`() {
        val files = createTempDirectory("creation-corrupt-state").toFile()
        val history = File(files, "creation/history.json").apply {
            requireNotNull(parentFile).mkdirs()
            writeText("{not-an-array}")
        }

        assertFalse(creationDurableStateIsReadable(files))

        history.delete()
        history.parentFile?.delete()
        files.delete()
    }

    @Test
    fun `isolated cleanup never overwrites a replacement and can restore exact bytes`() {
        val root = createTempDirectory("creation-cleanup-root").toFile()
        val original = File(root, "artifact.bin").apply { writeText("original") }
        val isolation = requireNotNull(planCreationFileIsolation(root, original))
        assertTrue(isolateCreationFileConfined(root, isolation))
        val replacement = File(root, "artifact.bin").apply { writeText("replacement") }

        assertFalse(restoreCreationFileConfined(root, isolation))
        assertEquals("replacement", replacement.readText())
        assertEquals("original", isolation.isolated.readText())

        assertTrue(replacement.delete())
        assertTrue(restoreCreationFileConfined(root, isolation))
        assertEquals("original", original.readText())

        original.delete()
        root.delete()
    }

    @Test
    fun `cleanup plan is recoverable both before and after the atomic move`() {
        val root = createTempDirectory("creation-cleanup-crash").toFile()
        val original = File(root, "artifact.bin").apply { writeText("bytes") }
        val isolation = requireNotNull(planCreationFileIsolation(root, original))

        assertTrue(original.isFile)
        assertFalse(isolation.isolated.exists())
        assertTrue(isolateCreationFileConfined(root, isolation))
        assertFalse(original.exists())
        assertEquals("bytes", isolation.isolated.readText())
        assertTrue(restoreCreationFileConfined(root, isolation))
        assertEquals("bytes", original.readText())

        original.delete()
        root.delete()
    }

    @Test
    fun `staging seal rejects same-directory and external symbolic links`() {
        val root = createTempDirectory("creation-staging-root").toFile()
        val outside = createTempDirectory("creation-staging-outside").toFile()
        val sameTarget = File(root, "same-target.bin").apply { writeText("same") }
        val externalTarget = File(outside, "external.bin").apply { writeText("external") }
        val sameLink = File(root, "same-link.bin")
        val externalLink = File(root, "external-link.bin")
        val linked = runCatching {
            Files.createSymbolicLink(sameLink.toPath(), sameTarget.toPath())
            Files.createSymbolicLink(externalLink.toPath(), externalTarget.toPath())
            true
        }.getOrDefault(false)
        if (linked) {
            assertEquals(null, sealCreationStagingFile(root, sameLink))
            assertEquals(null, sealCreationStagingFile(root, externalLink))
            assertEquals("same", sameTarget.readText())
            assertEquals("external", externalTarget.readText())
        }
        deleteCreationTreeNoFollow(requireNotNull(root.parentFile), root)
        externalTarget.delete()
        outside.delete()
    }

    @Test
    fun `staging seal keeps the journal-owned exact regular entry stable`() {
        val root = createTempDirectory("creation-staging-seal").toFile()
        val original = File(root, "result.bin").apply { writeText("validated") }

        val sealed = requireNotNull(sealCreationStagingFile(root, original))

        assertEquals(original.absolutePath, sealed.absolutePath)
        assertTrue(original.exists())
        assertEquals("validated", sealed.readText())
        sealed.delete()
        root.delete()
    }

    @Test
    fun `parallel same-source staging reservations freeze distinct exact identities`() {
        val root = createTempDirectory("creation-staging-parallel").toFile()
        val existing = File(root, "same.svg").apply { writeText("external") }
        val reserved = java.util.Collections.synchronizedList(mutableListOf<File>())
        val threads = (0 until 8).map {
            thread { reserved += reserveCreationStagingFile(root, "same.svg") }
        }
        threads.forEach(Thread::join)

        assertEquals(8, reserved.map(File::getAbsolutePath).distinct().size)
        assertTrue(reserved.all(File::isFile))
        assertEquals("external", existing.readText())

        reserved.forEach(File::delete)
        existing.delete()
        root.delete()
    }

    @Test
    fun `managed publishing preserves an existing target and isolates parallel names`() {
        val files = createTempDirectory("creation-publish-parallel").toFile()
        val staging = File(files, "creation/staging/svg").apply { mkdirs() }
        val library = File(files, "creation/library").apply { mkdirs() }
        val existing = File(library, "result.svg").apply { writeText("external") }
        val sources = (0 until 6).map { index ->
            File(staging, "$index.svg").apply { writeText("result-$index") }
        }
        val published = java.util.Collections.synchronizedList(mutableListOf<File>())
        val threads = sources.map { source ->
            thread { published += publishManagedCreationResult(files, source, "result.svg") }
        }
        threads.forEach(Thread::join)

        assertEquals("external", existing.readText())
        assertEquals(6, published.map(File::getAbsolutePath).distinct().size)
        assertEquals((0 until 6).map { "result-$it" }.toSet(), published.map(File::readText).toSet())

        deleteCreationTreeNoFollow(requireNotNull(files.parentFile), files)
    }

    @Test
    fun `delivery recovery commits only a size and digest verified receipt`() {
        assertEquals(
            CreationDeliveryRecoveryAction.PUBLISH_SEALED,
            decideCreationDeliveryRecovery(
                sealedMatchesReceipt = true,
                publishedExists = false,
                publishedMatchesReceipt = false,
            ),
        )
        assertEquals(
            CreationDeliveryRecoveryAction.COMMIT_VERIFIED,
            decideCreationDeliveryRecovery(
                sealedMatchesReceipt = true,
                publishedExists = true,
                publishedMatchesReceipt = true,
            ),
        )
        assertEquals(
            CreationDeliveryRecoveryAction.WAIT_FOR_OWNED_BYTES,
            decideCreationDeliveryRecovery(
                sealedMatchesReceipt = true,
                publishedExists = true,
                publishedMatchesReceipt = false,
            ),
        )
    }

    @Test
    fun `delivery planning never reuses an occupied provider name`() {
        assertEquals(
            "result.svg",
            uniqueCreationDeliveryName("result.svg", emptySet(), "dispatch-a"),
        )
        val collision = uniqueCreationDeliveryName(
            "result.svg",
            setOf("result.svg"),
            "dispatch-a",
        )
        assertTrue(collision != "result.svg")
        assertTrue(collision.endsWith(".svg"))
    }

    private fun retentionItem(
        id: String,
        createdAtMs: Long,
        vararg paths: String,
    ) = CreationHistoryRetentionItem(
        id = id,
        tool = "image",
        createdAtMs = createdAtMs,
        managedPaths = paths.toSet(),
    )
}
