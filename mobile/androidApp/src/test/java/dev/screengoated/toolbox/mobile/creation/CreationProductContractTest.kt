package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationProductContractTest {
    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun `release packages only the active 3d creation capability`() {
        val active = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
            .objectAt("runtimeCapabilities")
            .getValue("deliveredFeatures")
            .jsonArray
            .map { it.jsonPrimitive.content }
        assertEquals(listOf("image_to_3d"), active)

        listOf(
            "parity-fixtures/image-to-svg/state-contract.json",
            "parity-fixtures/image-creation-editing/state-contract.json",
        ).forEach { path ->
            val release = loadFixture(path).objectAt("releaseAvailability")
            assertEquals("archived_source_only", release.stringAt("packaging"))
            assertFalse(release.booleanAt("compiledHostCode"))
            assertFalse(release.booleanAt("embeddedFrontendAssets"))
            assertFalse(release.booleanAt("runtimeCapabilityDelivered"))
        }
    }

    @Test
    fun `creation distributions expose only shared product delivery invariants`() {
        val expected = setOf(
            "fullSupported",
            "playSupported",
            "behaviorIdentical",
            "featureSetIdentical",
            "integrityValidatedBeforeUse",
        )
        val fixtures = listOf(
            Triple("parity-fixtures/image-to-3d/state-contract.json", "schemaVersion", 35),
            Triple("parity-fixtures/image-to-svg/state-contract.json", "schemaVersion", 32),
            Triple(
                "parity-fixtures/image-creation-editing/state-contract.json",
                "fixtureVersion",
                46,
            ),
        )
        fixtures.forEach { (path, versionField, version) ->
            val fixture = loadFixture(path)
            val distribution = fixture.objectAt("distribution")
            val publicBoundary = fixture.objectAt("publicBoundary")

            assertEquals(expected, distribution.keys)
            assertTrue(expected.all { distribution.booleanAt(it) })
            assertTrue(publicBoundary.booleanAt("containsProductContractOnly"))
            assertFalse(publicBoundary.booleanAt("implementationDetailsVisible"))
            assertEquals(version, fixture.intAt(versionField))
        }
    }

    @Test
    fun `creation histories present newest result first`() {
        listOf(
            "parity-fixtures/image-to-3d/state-contract.json",
            "parity-fixtures/image-to-svg/state-contract.json",
            "parity-fixtures/image-creation-editing/state-contract.json",
        ).forEach { path ->
            assertEquals(
                "newest_session_first",
                loadFixture(path).objectAt("history").stringAt("presentationOrder"),
            )
            val history = loadFixture(path).objectAt("history")
            assertFalse(history.booleanAt("singleDeleteRequiresConfirmation"))
            assertTrue(history.booleanAt("deleteAllRequiresConfirmation"))
            assertEquals(
                "current_tool_saved_results",
                history.stringAt("deleteAllScope"),
            )
        }
    }

    @Test
    fun `creation readiness expands without blocking ready work`() {
        listOf(
            "parity-fixtures/image-to-3d/state-contract.json",
            "parity-fixtures/image-to-svg/state-contract.json",
            "parity-fixtures/image-creation-editing/state-contract.json",
        ).forEach { path ->
            val readiness = loadFixture(path).objectAt("readiness")
            assertTrue(readiness.booleanAt("immediateReserveCoversParallelLimit"))
            assertTrue(readiness.booleanAt("acceptedDemandExpandsPreparation"))
            assertTrue(readiness.booleanAt("expansionIsBounded"))
            assertTrue(readiness.booleanAt("consumptionStartsBackgroundReplenishment"))
            assertTrue(readiness.booleanAt("readyWorkDoesNotWaitForReplenishment"))
            assertTrue(readiness.booleanAt("toolCapacityIsIsolated"))
        }
    }

    @Test
    fun `creation surfaces do no recurring work while idle`() {
        listOf(
            "parity-fixtures/image-to-3d/state-contract.json",
            "parity-fixtures/image-to-svg/state-contract.json",
            "parity-fixtures/image-creation-editing/state-contract.json",
        ).forEach { path ->
            val activity = loadFixture(path).objectAt("surfaceActivity")
            assertTrue(activity.booleanAt("firstPaintPrecedesReadinessRequest"))
            assertFalse(activity.booleanAt("idleJobStatusPolling"))
            assertFalse(activity.booleanAt("idleHistoryPolling"))
            assertFalse(activity.booleanAt("idleReadinessPolling"))
            assertEquals(
                "accepted_or_recovered_work",
                activity.stringAt("jobStatusPollingScope"),
            )
            assertEquals("active_work", activity.stringAt("estimatedProgressTickScope"))
        }
    }

    @Test
    fun `creation recovery remains durable and fail closed`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val recovery = fixture.objectAt("recovery")

        assertTrue(recovery.booleanAt("intentRecordedBeforeSubmit"))
        assertTrue(recovery.booleanAt("dispatchIdentityGeneratedByHost"))
        assertTrue(recovery.booleanAt("contentFingerprintIsNotDispatchIdentity"))
        assertTrue(recovery.booleanAt("replayMatchesOnlySameDispatchId"))
        assertTrue(recovery.booleanAt("acceptedJobResumedWithoutResubmit"))
        assertTrue(recovery.booleanAt("unknownSubmissionFailsClosed"))
        assertTrue(recovery.booleanAt("artifactCommittedBeforeHostSuccess"))
        assertEquals(7 * 24, recovery.intAt("recoveryRetentionHours"))
    }

    @Test
    fun `host lifecycle and measured timing guarantees stay explicit`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val lifecycle = fixture.objectAt("hostLifecycle")
        val timing = fixture.objectAt("timing")

        assertTrue(lifecycle.booleanAt("executionLossFailsActiveJobExactlyOnce"))
        assertTrue(lifecycle.booleanAt("staleCompletionCannotReleaseNewExecution"))
        assertTrue(lifecycle.booleanAt("cancelWinsOverLateSuccess"))
        assertTrue(lifecycle.booleanAt("lateSuccessCannotPublishAfterCancel"))
        assertTrue(lifecycle.booleanAt("lateSuccessCannotDeletePreviousOutputAfterCancel"))
        assertTrue(lifecycle.booleanAt("multipleSessionsPerOwnerSupported"))
        assertTrue(lifecycle.booleanAt("closeCancelsOnlyOwnerJobs"))
        assertTrue(lifecycle.booleanAt("closeReleasesOwnerExecutionResources"))
        assertTrue(lifecycle.booleanAt("closeDestroysProductSurface"))
        val svgLifecycle = loadFixture(
            "parity-fixtures/image-to-svg/state-contract.json",
        ).objectAt("hostLifecycle")
        assertTrue(svgLifecycle.booleanAt("cancelWinsOverLateSuccess"))
        assertTrue(svgLifecycle.booleanAt("multipleSessionsPerOwnerSupported"))
        assertTrue(svgLifecycle.booleanAt("closeCancelsOnlyOwnerJobs"))
        assertTrue(svgLifecycle.booleanAt("closeReleasesOwnerExecutionResources"))
        assertTrue(svgLifecycle.booleanAt("closeDestroysProductSurface"))
        assertTrue(timing.booleanAt("persistMeasuredDurations"))
        assertTrue(timing.booleanAt("boundedSamples"))
        assertTrue(timing.booleanAt("reportSampleCount"))
        assertTrue(timing.booleanAt("fixedEstimateOnlyWhenSampleCountIsZero"))
    }

    @Test
    fun `draft and terminal items are configurable while submitted items remain immutable`() {
        fun item(stage: CreationNativeStage, submitted: Boolean = true) = CreationNativeItem(
            id = "item",
            batchId = "batch",
            sourcePath = "source.png",
            sourceName = "source.png",
            stage = stage,
            submitted = submitted,
        )

        assertTrue(item(CreationNativeStage.DRAFT, submitted = false).isConfigurable())
        assertTrue(item(CreationNativeStage.DONE).isConfigurable())
        assertTrue(item(CreationNativeStage.FAILED).isConfigurable())
        assertTrue(item(CreationNativeStage.CANCELLED).isConfigurable())
        assertFalse(item(CreationNativeStage.QUEUED).isConfigurable())
        assertFalse(item(CreationNativeStage.RUNNING).isConfigurable())
        assertFalse(item(CreationNativeStage.DRAFT).isConfigurable())
    }

    @Test
    fun `primary action submits only the selected session from an imported batch`() {
        fun draft(id: String) = CreationNativeItem(
            id = id,
            batchId = "shared-import",
            sourcePath = "$id.png",
            sourceName = "$id.png",
        )
        val submitted = CreationNativeUiState(
            items = listOf(draft("first"), draft("selected"), draft("last")),
            selectedItemId = "selected",
        ).submitSelectedItem()

        assertFalse(submitted.items[0].submitted)
        assertEquals(CreationNativeStage.DRAFT, submitted.items[0].stage)
        assertFalse(submitted.items[1].submitted)
        assertEquals(CreationNativeStage.DRAFT, submitted.items[1].stage)
        assertFalse(submitted.items[2].submitted)
        assertEquals(CreationNativeStage.DRAFT, submitted.items[2].stage)
        assertEquals("selected.png", submitted.items[3].sourcePath)
        assertEquals(CreationNativeStage.QUEUED, submitted.items[3].stage)
    }

    @Test
    fun `repeated 3d and svg imports remain separate independently submitted sessions`() {
        listOf(CreationTool.IMAGE_TO_3D, CreationTool.IMAGE_TO_SVG).forEach { tool ->
            val drafts = creationDraftsForImport(
                paths = listOf("same.png", "same.png"),
                batchId = "${tool.wireName}-batch",
                idForIndex = { index -> "${tool.wireName}-$index" },
            )
            assertEquals(listOf("same.png", "same.png"), drafts.map(CreationNativeItem::sourcePath))
            assertTrue(drafts[0].id != drafts[1].id)

            val first = CreationNativeUiState(
                items = drafts,
                selectedItemId = drafts[0].id,
            ).submitSelectedItem(submissionToken = "first")
            val second = first.copy(selectedItemId = drafts[1].id)
                .submitSelectedItem(submissionToken = "second")

            assertEquals(CreationNativeStage.DRAFT, second.items[0].stage)
            assertEquals(CreationNativeStage.DRAFT, second.items[1].stage)
            assertEquals(
                listOf("first", "second"),
                second.items.mapNotNull(CreationNativeItem::submissionToken),
            )
        }
    }

    @Test
    fun `every deliberate rapid press clones one selected session with a fresh token`() {
        val draft = CreationNativeItem("draft", "batch", "one.png", "one.png")
        val first = CreationNativeUiState(
            items = listOf(draft),
            selectedItemId = draft.id,
        ).submitSelectedItem("job-1", "batch-1", "press-1")
        val second = first.submitSelectedItem("job-2", "batch-2", "press-2")
        val third = second.submitSelectedItem("job-3", "batch-3", "press-3")
        val duplicateCallback = third.submitSelectedItem("wrong", "wrong", "press-3")

        assertEquals(listOf("draft", "job-1", "job-2", "job-3"), third.items.map { it.id })
        assertEquals(3, third.items.count { it.stage == CreationNativeStage.QUEUED })
        assertEquals(third, duplicateCallback)
    }

    @Test
    fun `terminal retry clones selected result for every tool and terminal state`() {
        CreationTool.entries.forEach { tool ->
            CreationNativeStage.entries.filter(CreationNativeStage::isTerminal).forEach { terminal ->
                val selected = CreationNativeItem(
                    id = "selected",
                    batchId = "batch",
                    sourcePath = "source.png",
                    sourceName = "source.png",
                    submitted = true,
                    stage = terminal,
                    status = CreationJobStatus(
                        jobId = "completed",
                        stage = terminal.name.lowercase(),
                        progressText = "complete",
                        outputPath = "content://media/result",
                    ),
                )
                val untouched = selected.copy(id = "other")
                val state = CreationNativeUiState(
                    items = listOf(selected, untouched),
                    selectedItemId = selected.id,
                ).submitSelectedItem(
                    newItemId = "${tool.wireName}-retry",
                    newBatchId = "${tool.wireName}-batch",
                    submissionToken = "${tool.wireName}-token",
                )

                assertEquals(selected, state.items[0])
                assertEquals(terminal, state.items[1].stage)
                val retry = state.items[2]
                assertEquals("${tool.wireName}-retry", retry.id)
                assertEquals("${tool.wireName}-token", retry.submissionToken)
                assertEquals(CreationNativeStage.QUEUED, retry.stage)
                assertEquals(null, retry.status)
                assertEquals(retry.id, state.selectedItemId)
            }
        }
    }

    @Test
    fun `visible terminal session retains its source for generate again`() {
        val terminal = CreationNativeItem(
            id = "terminal",
            batchId = "batch",
            sourcePath = "creation/sources/original.png",
            sourceName = "original.png",
            submitted = true,
            stage = CreationNativeStage.DONE,
        )
        val state = CreationNativeUiState(
            items = listOf(terminal),
            selectedItemId = terminal.id,
        )

        assertEquals(
            setOf("creation/sources/original.png"),
            creationVisibleSessionSourceHandles(state.items),
        )
        assertEquals(
            terminal.sourcePath,
            state.submitSelectedItem("retry", "retry-batch", "retry-press")
                .items.last().sourcePath,
        )
    }

    @Test
    fun `cancelled terminal state cannot accept a late completion`() {
        assertTrue(creationStageIsBusy("generating"))
        assertTrue(creationStageIsBusy("finalizing"))
        assertFalse(creationStageIsBusy("cancelled"))
        assertFalse(creationStageIsBusy("done"))
    }

    @Test
    fun `continuation expires in memory at the same boundary as recovery`() {
        val now = 1_000_000L
        val lifetime = 24L * 60 * 60 * 1_000

        assertTrue(creationContinuationIsLive(now - lifetime, now, lifetime))
        assertFalse(creationContinuationIsLive(now - lifetime - 1, now, lifetime))
        assertFalse(creationContinuationIsLive(now + 1, now, lifetime))
    }

    private fun loadFixture(path: String): JsonObject =
        json.parseToJsonElement(File(repoRoot(), path).readText()).jsonObject

    private fun repoRoot(): File {
        val workingDirectory =
            File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        return generateSequence(workingDirectory) { it.parentFile }
            .take(8)
            .map { it.canonicalFile }
            .firstOrNull { root -> File(root, "parity-fixtures").isDirectory }
            ?: error("Could not locate the repository from $workingDirectory")
    }

    private fun JsonObject.objectAt(key: String) = requireNotNull(this[key]).jsonObject
    private fun JsonObject.intAt(key: String) = requireNotNull(this[key]).jsonPrimitive.int
    private fun JsonObject.stringAt(key: String) = requireNotNull(this[key]).jsonPrimitive.content
    private fun JsonObject.booleanAt(key: String) = requireNotNull(this[key]).jsonPrimitive.boolean
}
