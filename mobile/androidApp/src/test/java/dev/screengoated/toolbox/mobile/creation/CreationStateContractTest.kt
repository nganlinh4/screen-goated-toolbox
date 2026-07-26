package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
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
        val input = fixture.objectAt("input")
        val submission = fixture.objectAt("submission")
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
        assertEquals(1, input.intAt("minimumImagesPerJob"))
        assertEquals(1, input.intAt("maximumImagesPerJob"))
        assertTrue(input.booleanAt("multiplePickerImagesCreateIndependentJobs"))
        assertEquals("selected_session", submission.stringAt("primaryActionScope"))
        assertFalse(submission.booleanAt("submitsOtherSessions"))
        assertFalse(presentation.booleanAt("showImplementationBranding"))
        assertFalse(presentation.booleanAt("showImplementationSelection"))
        assertTrue(presentation.booleanAt("showGenerationModeSelection"))
        assertTrue(presentation.booleanAt("normalizeImplementationText"))
        assertEquals("Material Symbols Rounded", presentation.stringAt("iconFamily"))
        assertEquals(1, presentation.intAt("iconFill"))
        assertTrue(presentation.booleanAt("sharedIconCatalog"))
        assertTrue(presentation.booleanAt("unchangedPollPreservesQueueDom"))
        assertTrue(presentation.booleanAt("hoveredSelectionTargetSurvivesPolling"))
        assertTrue(presentation.booleanAt("pointerSequenceSurvivesPolling"))
        assertTrue(presentation.booleanAt("queueOwnsOverflow"))
        assertTrue(presentation.booleanAt("primaryActionRemainsReachable"))
        val preview = presentation.objectAt("previewMemory")
        assertFalse(preview.booleanAt("webviewRetainsOriginalImageBytes"))
        assertFalse(preview.booleanAt("decodeBlocksWebviewThread"))
        assertFalse(preview.booleanAt("offscreenPreviewsHydrate"))
        assertTrue(preview.booleanAt("selectedPreviewHasPriority"))
        assertTrue(preview.booleanAt("backgroundHydrationYieldsToInteraction"))
        assertEquals(128, preview.intAt("thumbnailMaximumEdgePixels"))
        assertEquals(1_600, preview.intAt("stageMaximumEdgePixels"))
        assertTrue(segmentation.booleanAt("requireRenderableNormals"))
        assertTrue(segmentation.booleanAt("expandDisconnectedComponentsForSingleMesh"))
        assertTrue(segmentation.booleanAt("preserveExistingPartNodes"))
        assertTrue(segmentation.booleanAt("windowsViewerRepairsLegacySegmentedOutputs"))
        assertEquals(24, segmentation.intAt("continuationWindowHours"))
        assertTrue(segmentation.booleanAt("newGenerationMayInvalidatePriorContinuation"))
        assertTrue(segmentation.booleanAt("fastResultIsSegmented"))
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
        val input = fixture.objectAt("input")
        val submission = fixture.objectAt("submission")
        val models = fixture.objectAt("models")
        val surface = fixture.objectAt("androidSurface")
        val preview = fixture.objectAt("previewMemory")
        val presentation = fixture.objectAt("presentation")
        val viewer = fixture.objectAt("viewer")

        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.intAt("maximumParallelJobs"))
        assertEquals(1, input.intAt("minimumImagesPerJob"))
        assertEquals(1, input.intAt("maximumImagesPerJob"))
        assertTrue(input.booleanAt("multiplePickerImagesCreateIndependentJobs"))
        assertEquals("selected_session", submission.stringAt("primaryActionScope"))
        assertFalse(submission.booleanAt("submitsOtherSessions"))
        assertEquals(setOf("simple", "detail"), models.keys)
        assertTrue(models.objectAt("simple").booleanAt("selectable"))
        assertTrue(models.objectAt("detail").booleanAt("selectable"))
        assertFalse(preview.booleanAt("webviewRetainsOriginalImageBytes"))
        assertFalse(preview.booleanAt("decodeBlocksWebviewThread"))
        assertFalse(preview.booleanAt("offscreenPreviewsHydrate"))
        assertTrue(preview.booleanAt("selectedPreviewHasPriority"))
        assertTrue(preview.booleanAt("backgroundHydrationYieldsToInteraction"))
        assertEquals(128, preview.intAt("thumbnailMaximumEdgePixels"))
        assertEquals(1_600, preview.intAt("stageMaximumEdgePixels"))
        assertEquals("Material Symbols Rounded", presentation.stringAt("iconFamily"))
        assertEquals(1, presentation.intAt("iconFill"))
        assertTrue(presentation.booleanAt("sharedIconCatalog"))
        assertTrue(presentation.booleanAt("unchangedPollPreservesQueueDom"))
        assertTrue(presentation.booleanAt("hoveredSelectionTargetSurvivesPolling"))
        assertTrue(presentation.booleanAt("pointerSequenceSurvivesPolling"))
        assertTrue(presentation.booleanAt("queueOwnsOverflow"))
        assertTrue(presentation.booleanAt("primaryActionRemainsReachable"))
        assertTrue(viewer.booleanAt("allPathsRendered"))
        assertFalse(viewer.booleanAt("animateAllPaths"))
        assertEquals(120, viewer.intAt("maximumAnimatedPaths"))
        assertTrue(viewer.booleanAt("adaptiveOverlappingAnimation"))
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
    fun `image creator native shell preserves canonical product contract`() {
        val fixture = loadFixture(
            "parity-fixtures/image-creation-editing/state-contract.json",
        )
        val request = fixture.objectAt("request")
        val submission = fixture.objectAt("submission")
        val prompt = request.objectAt("prompt")
        val references = request.objectAt("references")
        val copyPolicy = fixture.objectAt("publicCopyPolicy")
        val artifact = fixture.objectAt("artifact")
        val locales = fixture.objectAt("locales")
        val presentation = fixture.objectAt("presentation")
        val behavior = fixture.objectAt("behavior")
        val surface = fixture.objectAt("androidSurface")
        val stages = fixture.getValue("publicStages").jsonArray
            .map { it.jsonPrimitive.content }
            .toSet()

        assertEquals("image", fixture.stringAt("tool"))
        assertEquals("image_", fixture.stringAt("jobIdPrefix"))
        assertEquals(CreationContract.IMAGE_CREATOR_OPERATION, fixture.stringAt("operation"))
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_PARALLEL_JOBS,
            fixture.intAt("maximumParallelJobs"),
        )
        assertEquals(
            CreationContract.IMAGE_CREATOR_WORKSPACES,
            fixture.intAt("preparedWorkspaces"),
        )
        assertEquals(
            CreationContract.MAXIMUM_CONCURRENT_PREPARATIONS,
            fixture.intAt("maximumConcurrentPreparations"),
        )
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_PROMPT_CHARACTERS,
            prompt.intAt("maximumCharacters"),
        )
        assertEquals(1, prompt.intAt("minimumCharacters"))
        assertTrue(prompt.booleanAt("trimmed"))
        assertEquals(0, references.intAt("minimum"))
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES,
            references.intAt("maximum"),
        )
        assertTrue(references.booleanAt("ordered"))
        assertTrue(request.booleanAt("frozenBeforeQueue"))
        assertFalse(request.booleanAt("multipleInputsCreateIndependentJobs"))
        assertTrue(request.booleanAt("oneSessionCreatesOneJob"))
        assertTrue(request.booleanAt("plusCreatesEmptySession"))
        assertEquals("selected_session", submission.stringAt("primaryActionScope"))
        assertFalse(submission.booleanAt("submitsOtherSessions"))
        assertEquals("feature_only", copyPolicy.stringAt("vocabulary"))
        assertFalse(copyPolicy.booleanAt("implementationDetailsVisible"))
        assertFalse(copyPolicy.booleanAt("rawImplementationErrorsVisible"))
        assertTrue(copyPolicy.booleanAt("referenceUploadCopyRequiresReferences"))
        assertTrue(artifact.booleanAt("requiresDecodedDimensions"))
        assertTrue(artifact.booleanAt("requiresPositiveWidth"))
        assertTrue(artifact.booleanAt("requiresPositiveHeight"))
        assertTrue(artifact.booleanAt("atomicWrite"))
        assertEquals("image_to_svg_creation_shell", presentation.stringAt("windowsShell"))
        assertEquals("native_compose_m3e", presentation.stringAt("androidShell"))
        assertEquals("Google Sans Flex", presentation.stringAt("fontFamily"))
        assertEquals(100, presentation.intAt("fontRoundedAxis"))
        assertEquals("Material Symbols Rounded", presentation.stringAt("iconFamily"))
        assertEquals(1, presentation.intAt("iconFill"))
        assertFalse(presentation.booleanAt("appSpecificTheme"))
        assertTrue(presentation.booleanAt("sharedIconCatalog"))
        assertTrue(presentation.booleanAt("unchangedPollPreservesQueueDom"))
        assertTrue(presentation.booleanAt("hoveredSelectionTargetSurvivesPolling"))
        assertTrue(presentation.booleanAt("pointerSequenceSurvivesPolling"))
        assertTrue(presentation.booleanAt("singleClickStartsSubmission"))
        assertTrue(presentation.booleanAt("submissionLocksImmediately"))
        assertTrue(presentation.booleanAt("queueOwnsOverflow"))
        assertTrue(presentation.booleanAt("primaryActionRemainsReachable"))
        val estimatedProgress = presentation.objectAt("estimatedProgress")
        assertTrue(estimatedProgress.booleanAt("usesRuntimeEstimate"))
        assertTrue(estimatedProgress.booleanAt("usesElapsedTimeCurve"))
        assertTrue(estimatedProgress.booleanAt("monotonic"))
        assertEquals(0.94, estimatedProgress.doubleAt("maximumBeforeCompletion"), 0.0)
        assertEquals(1.0, estimatedProgress.doubleAt("completionRatio"), 0.0)
        assertTrue(estimatedProgress.booleanAt("showsLocalizedEta"))
        val preview = presentation.objectAt("previewMemory")
        assertFalse(preview.booleanAt("webviewRetainsOriginalImageBytes"))
        assertFalse(preview.booleanAt("decodeBlocksWebviewThread"))
        assertFalse(preview.booleanAt("offscreenPreviewsHydrate"))
        assertTrue(preview.booleanAt("selectedPreviewHasPriority"))
        assertTrue(preview.booleanAt("backgroundHydrationYieldsToInteraction"))
        assertEquals(128, preview.intAt("thumbnailMaximumEdgePixels"))
        assertEquals(1_600, preview.intAt("stageMaximumEdgePixels"))
        assertTrue(behavior.booleanAt("cancellationIsMonotonic"))
        assertTrue(behavior.booleanAt("lateSuccessCannotPublishAfterCancellation"))
        assertTrue(behavior.booleanAt("acceptedRequestIsNotRepeatedDuringRecovery"))
        assertTrue(behavior.booleanAt("retryCreatesNewJob"))
        assertTrue(behavior.booleanAt("retryPreservesPreviousResult"))
        assertTrue(behavior.booleanAt("closingUiCancelsToolJobs"))
        assertTrue(behavior.booleanAt("closingUiTerminatesTrackedProcessTrees"))
        assertTrue(behavior.booleanAt("closingUiDestroysWebSurface"))
        assertTrue(behavior.booleanAt("sharedPreparationSurvivesMiniAppClose"))
        assertTrue(behavior.booleanAt("failureRemainsBoundToJob"))
        assertEquals("adaptive_image_session_result", surface.stringAt("resultRenderer"))
        assertEquals(
            CreationContract.IMAGE_CREATOR_WORKSPACES,
            surface.intAt("isolatedWorkers"),
        )
        assertFalse(surface.booleanAt("implementationDetailsVisible"))
        assertEquals(
            setOf(
                "queued",
                "preparing",
                "uploading",
                "generating",
                "finalizing",
                "done",
                "failed",
                "cancelled",
            ),
            stages,
        )
        assertEquals(locales.stringAt("en"), MobileLocaleText.forLanguage("en").appImageCreatorTitle)
        assertEquals(locales.stringAt("ko"), MobileLocaleText.forLanguage("ko").appImageCreatorTitle)
        assertEquals(locales.stringAt("vi"), MobileLocaleText.forLanguage("vi").appImageCreatorTitle)
    }

    @Test
    fun `image creator exposes only normal product progress and failures`() {
        assertEquals("preparing", publicImageCreationStage("unknown"))
        assertEquals("Getting ready", publicImageCreationText("preparing"))
        assertEquals("Getting ready", publicImageCreationText("uploading", hasReferences = false))
        assertEquals(
            "Adding reference image",
            publicImageCreationText("uploading", hasReferences = true),
        )
        assertEquals(
            "Image creation could not finish. Try again.",
            publicImageCreationFailure(),
        )
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
    fun `creation recovery remains durable and fail closed`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val recovery = fixture.objectAt("recovery")

        assertTrue(recovery.booleanAt("intentRecordedBeforeSubmit"))
        assertTrue(recovery.booleanAt("jobIdentityUsesSourceContent"))
        assertTrue(recovery.booleanAt("jobIdentityIgnoresSourcePathAndTimestamps"))
        assertTrue(recovery.booleanAt("jobIdentityIncludesProductSettings"))
        assertTrue(recovery.booleanAt("matchingJobSerializedAcrossWorkers"))
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

        assertTrue(lifecycle.booleanAt("workerLossFailsActiveJobExactlyOnce"))
        assertTrue(lifecycle.booleanAt("staleCallbackCannotReleaseNewAssignment"))
        assertTrue(lifecycle.booleanAt("cancelWinsOverLateSuccess"))
        assertTrue(lifecycle.booleanAt("lateSuccessCannotPublishAfterCancel"))
        assertTrue(lifecycle.booleanAt("lateSuccessCannotDeletePreviousOutputAfterCancel"))
        assertTrue(lifecycle.booleanAt("closeCancelsToolJobs"))
        assertTrue(lifecycle.booleanAt("closeTerminatesTrackedProcessTrees"))
        assertTrue(lifecycle.booleanAt("closeDestroysWebSurface"))
        assertTrue(lifecycle.booleanAt("sharedPreparationSurvivesMiniAppClose"))
        val svgLifecycle = loadFixture(
            "parity-fixtures/image-to-svg/state-contract.json",
        ).objectAt("hostLifecycle")
        assertTrue(svgLifecycle.booleanAt("cancelWinsOverLateSuccess"))
        assertTrue(svgLifecycle.booleanAt("closeCancelsToolJobs"))
        assertTrue(svgLifecycle.booleanAt("closeTerminatesTrackedProcessTrees"))
        assertTrue(svgLifecycle.booleanAt("closeDestroysWebSurface"))
        assertTrue(svgLifecycle.booleanAt("sharedPreparationSurvivesMiniAppClose"))
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
        assertTrue(submitted.items[1].submitted)
        assertEquals(CreationNativeStage.QUEUED, submitted.items[1].stage)
        assertFalse(submitted.items[2].submitted)
        assertEquals(CreationNativeStage.DRAFT, submitted.items[2].stage)
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
    private fun JsonObject.doubleAt(key: String) = requireNotNull(this[key]).jsonPrimitive.double
    private fun JsonObject.stringAt(key: String) = requireNotNull(this[key]).jsonPrimitive.content
    private fun JsonObject.booleanAt(key: String) = requireNotNull(this[key]).jsonPrimitive.boolean
}
