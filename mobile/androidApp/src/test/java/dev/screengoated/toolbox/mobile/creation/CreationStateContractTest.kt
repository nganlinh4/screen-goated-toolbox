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

class CreationStateContractTest {
    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun `image to 3d native shell preserves canonical limits`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val defaults = fixture.objectAt("defaults")
        val limits = fixture.objectAt("limits")
        val presentation = fixture.objectAt("presentation")
        val segmentation = fixture.objectAt("segmentation")
        val history = fixture.objectAt("history")
        val surface = fixture.objectAt("androidSurface")
        val generationPrerequisite = segmentation.objectAt("generationPrerequisite")

        assertEquals(CreationContract.DEFAULT_POLYCOUNT, defaults.intAt("polycount"))
        assertEquals(
            CreationGenerationMode.QUALITY.wireName,
            defaults.stringAt("generationMode"),
        )
        assertFalse(defaults.booleanAt("autoSegment"))
        assertEquals(CreationContract.MINIMUM_POLYCOUNT, limits.intAt("minimumPolycount"))
        assertEquals(CreationContract.MAXIMUM_POLYCOUNT, limits.intAt("maximumPolycount"))
        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.intAt("maximumParallelJobs"))
        assertFalse(presentation.booleanAt("showProviderBranding"))
        assertFalse(presentation.booleanAt("showProviderSelection"))
        assertTrue(presentation.booleanAt("showGenerationModeSelection"))
        assertTrue(presentation.booleanAt("sanitizeProviderBrandedRuntimeText"))
        assertTrue(segmentation.booleanAt("requireRenderableNormals"))
        assertTrue(segmentation.booleanAt("expandDisconnectedComponentsForSingleMesh"))
        assertTrue(segmentation.booleanAt("preserveExistingPartNodes"))
        assertTrue(segmentation.booleanAt("windowsViewerRepairsLegacySegmentedOutputs"))
        assertFalse(segmentation.booleanAt("continuationRequiresGenerationCredits"))
        assertFalse(segmentation.booleanAt("continuationRequiresGenerationReadiness"))
        assertTrue(segmentation.booleanAt("continuationUsesOwningWorkspace"))
        assertTrue(segmentation.booleanAt("insufficientCreditsDoNotResetContinuationOwner"))
        assertTrue(segmentation.booleanAt("newGenerationInvalidatesReusedWorkspaceContinuation"))
        assertEquals(
            "validated_artifact",
            generationPrerequisite.stringAt("unsegmentedResult"),
        )
        assertEquals(
            "validated_artifact_and_terminal_success",
            generationPrerequisite.stringAt("automaticSeparation"),
        )
        assertEquals(
            "validated_artifact_and_terminal_success",
            generationPrerequisite.stringAt("continuation"),
        )
        assertTrue(history.booleanAt("retainCurrentSessionResultUntilHistoryPersists"))
        assertTrue(history.booleanAt("doneStatusTransitionsDirectlyToModel"))
        assertTrue(history.booleanAt("freezeGenerationMode"))
        assertTrue(history.booleanAt("freezeProvider"))
        assertTrue(history.booleanAt("terminalItemCanBeReconfiguredAndRerun"))
        assertTrue(history.booleanAt("rerunPreservesPreviousOutput"))
        assertEquals("native_compose_m3e", surface.stringAt("shell"))
        assertEquals("sceneview_filament", surface.stringAt("resultRenderer"))
        assertEquals("depth_anything_3_relief", surface.stringAt("progressPreview"))
        assertEquals(DepthPreviewContract.INPUT_SIDE, surface.intAt("previewInputSide"))
        assertFalse(surface.booleanAt("previewBlocksGeneration"))
        assertFalse(surface.booleanAt("previewSetupVisible"))
        assertEquals(18, surface.intAt("preparationProgressMaximumPercent"))
        assertEquals("bounded_privacy_safe_journal", surface.stringAt("diagnostics"))
        assertFalse(surface.booleanAt("backgroundAutomationVisible"))
        assertTrue(surface.booleanAt("wireframeAndOutlineIndependent"))
        val viewerControls = surface.getValue("viewerControls").jsonArray
            .map { it.jsonPrimitive.content }
            .toSet()
        assertTrue(
            viewerControls.containsAll(
                setOf("orbit", "zoom", "pan", "grid", "wireframe", "auto_rotate", "toon", "outline"),
            ),
        )
    }

    @Test
    fun `image to svg native shell preserves canonical limits`() {
        val fixture = loadFixture("parity-fixtures/image-to-svg/state-contract.json")
        val limits = fixture.objectAt("limits")
        val models = fixture.objectAt("models")
        val surface = fixture.objectAt("androidSurface")

        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.intAt("maximumParallelJobs"))
        assertEquals(2, models.objectAt("simple").intAt("creditCost"))
        assertEquals(4, models.objectAt("detail").intAt("creditCost"))
        assertEquals("native_compose_m3e", surface.stringAt("shell"))
        assertEquals("sandboxed_svg_document", surface.stringAt("resultRenderer"))
        assertEquals("depth_anything_3_six_bins", surface.stringAt("progressPreview"))
        assertEquals(DepthPreviewContract.INPUT_SIDE, surface.intAt("previewInputSide"))
        assertFalse(surface.booleanAt("previewBlocksGeneration"))
        assertFalse(surface.booleanAt("previewSetupVisible"))
        assertEquals(18, surface.intAt("preparationProgressMaximumPercent"))
        assertEquals("bounded_privacy_safe_journal", surface.stringAt("diagnostics"))
        assertFalse(surface.booleanAt("backgroundAutomationVisible"))
    }

    @Test
    fun `android depth preview uses the canonical windows model`() {
        val windowsSource = File(
            repoRoot(),
            "src/overlay/three_d_generator/depth_model.rs",
        ).readText()

        assertTrue(windowsSource.contains(DepthPreviewContract.MODEL_URL))
        assertTrue(windowsSource.replace("_", "").contains(DepthPreviewContract.MODEL_BYTES.toString()))
        assertTrue(windowsSource.contains(DepthPreviewContract.MODEL_SHA256))
        assertTrue(windowsSource.contains("const SIDE: u32 = ${DepthPreviewContract.INPUT_SIDE};"))
    }

    @Test
    fun `mailbox preparation uses patient capped retries`() {
        assertEquals(4, CreationContract.IMAGE_TO_3D_WORKSPACES)
        assertEquals(1, CreationContract.MAXIMUM_CONCURRENT_PREPARATIONS)
        assertEquals(5 * 60_000L, CreationPreparationCooldown.mailboxFailureBackoffMs(1))
        assertEquals(10 * 60_000L, CreationPreparationCooldown.mailboxFailureBackoffMs(2))
        assertEquals(15 * 60_000L, CreationPreparationCooldown.mailboxFailureBackoffMs(3))
        assertEquals(15 * 60_000L, CreationPreparationCooldown.mailboxFailureBackoffMs(20))
    }

    @Test
    fun `creation runtime delivery remains flavor specific and integrity checked`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val distribution = fixture.objectAt("distribution")
        val full = distribution.objectAt("full")
        val fullIntegrity = full.objectAt("integrity")
        val play = distribution.objectAt("play")
        val playIntegrity = play.objectAt("integrity")

        assertEquals("identical", distribution.stringAt("hostBehavior"))
        assertTrue(distribution.booleanAt("sameRuntimeBuild"))
        assertTrue(distribution.booleanAt("sameRuntimeManifestVersionAndFeatures"))
        assertTrue(full.booleanAt("supported"))
        assertEquals("verified_download", full.stringAt("runtimeDelivery"))
        assertTrue(full.booleanAt("downloadExecutableCode"))
        assertTrue(fullIntegrity.booleanAt("bundleByteCountPinned"))
        assertTrue(fullIntegrity.booleanAt("bundleSha256Pinned"))
        assertTrue(fullIntegrity.booleanAt("extractedFileByteCountsPinned"))
        assertTrue(fullIntegrity.booleanAt("extractedFileSha256Pinned"))

        assertTrue(play.booleanAt("supported"))
        assertEquals("packaged_on_demand", play.stringAt("runtimeDelivery"))
        assertFalse(play.booleanAt("downloadExecutableCode"))
        assertFalse(play.booleanAt("networkExecutableFallback"))
        assertTrue(playIntegrity.booleanAt("playAppSigning"))
        assertTrue(playIntegrity.booleanAt("packagedArtifactSha256Pinned"))

        val fullSource = File(
            repoRoot(),
            "mobile/androidApp/src/full/java/dev/screengoated/toolbox/mobile/" +
                "creation/runtime/CreationRuntimeProvider.kt",
        ).readText()
        assertTrue(fullSource.contains("DexClassLoader"))
        assertTrue(fullSource.contains("BUNDLE_BYTES"))
        assertTrue(fullSource.contains("BUNDLE_SHA256"))
        assertTrue(fullSource.contains("DEX_SHA256"))
        assertTrue(fullSource.contains("NATIVE_SHA256"))

        val playSource = File(
            repoRoot(),
            "mobile/androidApp/src/play/java/dev/screengoated/toolbox/mobile/" +
                "creation/runtime/CreationRuntimeProvider.kt",
        ).readText()
        assertTrue(playSource.contains("SplitInstallRequest"))
        assertTrue(playSource.contains("feature_creation_runtime"))
        assertFalse(playSource.contains("DexClassLoader"))
        assertFalse(playSource.contains("OkHttpClient"))

        val complianceSource = File(
            repoRoot(),
            "mobile/androidApp/gradle/play-compliance.gradle.kts",
        ).readText()
        assertTrue(complianceSource.contains("feature_creation_runtime"))
        assertTrue(complianceSource.contains("Play creation native checksum mismatch"))
        assertTrue(complianceSource.contains("Play creation runtime feature is missing executable code"))
    }

    @Test
    fun `paid generation recovery remains durable and fail closed`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val recovery = fixture.objectAt("paidRecovery")

        assertTrue(recovery.booleanAt("generationIntentBeforeSubmit"))
        assertEquals(5, recovery.intAt("generationCostReserved"))
        assertTrue(recovery.booleanAt("jobIdentityUsesSourceContent"))
        assertTrue(recovery.booleanAt("jobIdentityIgnoresSourcePathAndTimestamps"))
        assertTrue(recovery.booleanAt("jobIdentityIncludesProviderAndMode"))
        assertTrue(recovery.booleanAt("jobIdentityIncludesPromptPolycountAndModel"))
        assertTrue(recovery.booleanAt("matchingJobSerializedAcrossWorkers"))
        assertTrue(recovery.booleanAt("acceptedTaskResumedWithoutResubmit"))
        assertTrue(recovery.booleanAt("unknownSubmissionFailsClosed"))
        assertTrue(recovery.booleanAt("qualityControlReadyBeforeSubmit"))
        assertTrue(recovery.booleanAt("qualityControlRequiresPointerEvents"))
        assertTrue(recovery.booleanAt("qualityControlRequiresTopmostHitTarget"))
        assertTrue(recovery.booleanAt("qualityAttemptPersistedBeforeClick"))
        assertTrue(recovery.booleanAt("qualityConfirmationPersistedSeparately"))
        assertTrue(recovery.booleanAt("qualityRecoveryUsesOwningAccount"))
        assertTrue(recovery.booleanAt("qualityTaskUrlConfirmsStart"))
        assertTrue(recovery.booleanAt("qualityGeneratingStateConfirmsStart"))
        assertTrue(recovery.booleanAt("qualityCreditDebitConfirmsStart"))
        assertEquals(
            "dom_click_while_control_remains_ready",
            recovery.stringAt("qualityNativeClickFallback"),
        )
        assertTrue(recovery.booleanAt("qualityUnknownSubmissionFailsClosed"))
        assertEquals("before_submission_only", recovery.stringAt("qualityRetryScope"))
        assertTrue(recovery.booleanAt("completionReceiptBeforeHostSuccess"))
        assertTrue(recovery.booleanAt("androidCompletionReceiptUsesPrivateRuntimeArtifact"))
        assertEquals(7 * 24, recovery.intAt("recoveryRetentionHours"))
        assertTrue(recovery.booleanAt("androidOwnerWorkerRedirect"))
        assertEquals("meshy-recovery-owner:", CreationContract.MESHY_RECOVERY_OWNER_PREFIX)
        assertEquals("quality-recovery-owner:", CreationContract.TRIPO_RECOVERY_OWNER_PREFIX)
    }

    @Test
    fun `host lifecycle and measured timing guarantees stay explicit`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val lifecycle = fixture.objectAt("hostLifecycle")
        val timing = fixture.objectAt("timing")

        assertTrue(lifecycle.booleanAt("workerLossFailsActiveJobExactlyOnce"))
        assertTrue(lifecycle.booleanAt("staleCallbackCannotReleaseNewAssignment"))
        assertTrue(lifecycle.booleanAt("cancelWinsOverLateSuccess"))
        assertTrue(lifecycle.booleanAt("lateSuccessCannotPublishAfterCancel"))
        assertTrue(lifecycle.booleanAt("lateSuccessCannotDeletePreviousOutputAfterCancel"))
        assertTrue(timing.booleanAt("persistMeasuredDurations"))
        assertTrue(timing.booleanAt("boundedSamples"))
        assertTrue(timing.booleanAt("reportSampleCount"))
        assertTrue(timing.booleanAt("fixedEstimateOnlyWhenSampleCountIsZero"))
    }

    @Test
    fun `terminal items rerun while active and queued items remain immutable`() {
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
    fun `cancelled terminal state cannot accept a late completion`() {
        assertTrue(creationStageIsBusy("generating"))
        assertTrue(creationStageIsBusy("finalizing"))
        assertFalse(creationStageIsBusy("cancelled"))
        assertFalse(creationStageIsBusy("done"))
    }

    private fun loadFixture(path: String): JsonObject {
        return json.parseToJsonElement(File(repoRoot(), path).readText()).jsonObject
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile
        }.firstOrNull { root -> File(root, "parity-fixtures").isDirectory }
            ?: error("Could not locate the repository from $workingDirectory")
    }

    private fun JsonObject.objectAt(key: String) = requireNotNull(this[key]).jsonObject
    private fun JsonObject.intAt(key: String) = requireNotNull(this[key]).jsonPrimitive.int
    private fun JsonObject.stringAt(key: String) = requireNotNull(this[key]).jsonPrimitive.content
    private fun JsonObject.booleanAt(key: String) = requireNotNull(this[key]).jsonPrimitive.boolean
}
