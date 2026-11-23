package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationStateContractTest {
    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun `cancellation commit preserves the latest state when unavailable`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val cancellation = fixture.objectAt("hostLifecycle").objectAt("cancellationCommit")
        assertFalse(cancellation.booleanAt("pendingDecisionStopsWorker"))
        assertFalse(cancellation.booleanAt("failedDecisionRestoresOldStatus"))
        assertTrue(cancellation.booleanAt("failedDecisionPreservesLatestWorkerState"))
        assertTrue(cancellation.booleanAt("durableCancellationStopsLatePublication"))
        assertTrue(cancellation.booleanAt("publicationWinnerRemainsDelivered"))
        assertTrue(cancellation.booleanAt("closeRequiresDurableDecisionBeforeComplete"))
    }

    @Test
    fun `image to 3d product contract preserves canonical limits`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val delivery = fixture.objectAt("runtimeDelivery")
        val states = fixture.getValue("states").jsonArray
            .map { it.jsonPrimitive.content }
            .toSet()
        val defaults = fixture.objectAt("defaults")
        val limits = fixture.objectAt("limits")
        val input = fixture.objectAt("input")
        val submission = fixture.objectAt("submission")
        val presentation = fixture.objectAt("presentation")
        val segmentation = fixture.objectAt("segmentation")
        val quadCompanion = fixture.objectAt("quadCompanion")
        val history = fixture.objectAt("history")
        val modelSafety = fixture.objectAt("modelSafety")
        val runtimeCapabilities = fixture.objectAt("runtimeCapabilities")
        val optionalInstruction = runtimeCapabilities.objectAt("optionalInstruction")
        val generationPrerequisite = segmentation.objectAt("generationPrerequisite")

        assertEquals("tracked-immutable-contract", delivery.stringAt("authority"))
        assertTrue(delivery.booleanAt("sharedAcrossWindowsAndAndroid"))
        assertTrue(delivery.booleanAt("requiresExactUrlVersionSizeAndSha256"))
        assertTrue(delivery.booleanAt("missingContractFailsBuild"))
        assertFalse(delivery.booleanAt("acceptsLocalRuntimeFallback"))
        assertFalse(delivery.booleanAt("acceptsEnvironmentOverride"))

        assertEquals(CreationContract.DEFAULT_POLYCOUNT, defaults.intAt("polycount"))
        assertEquals(
            CreationGenerationMode.QUALITY.wireName,
            defaults.stringAt("generationMode"),
        )
        assertFalse(defaults.booleanAt("autoSegment"))
        assertEquals(CreationContract.MINIMUM_POLYCOUNT, limits.intAt("minimumPolycount"))
        assertEquals(CreationContract.MAXIMUM_POLYCOUNT, limits.intAt("maximumPolycount"))
        assertEquals(
            CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES,
            limits.longAt("maximumResultBytes"),
        )
        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.intAt("maximumParallelJobs"))
        assertEquals(1, input.intAt("minimumImagesPerJob"))
        assertEquals(1, input.intAt("maximumImagesPerJob"))
        assertTrue(input.booleanAt("multiplePickerImagesCreateIndependentSessions"))
        assertInputImageContract(input)
        assertTrue("queued" in states)
        assertEquals("selected_session", submission.stringAt("primaryActionScope"))
        assertFalse(submission.booleanAt("submitsOtherSessions"))
        assertTrue(submission.booleanAt("explicitSubmissionCreatesFreshDispatchId"))
        assertFalse(presentation.booleanAt("showImplementationBranding"))
        assertFalse(presentation.booleanAt("showImplementationSelection"))
        assertTrue(presentation.booleanAt("showGenerationModeSelection"))
        assertTrue(presentation.booleanAt("normalizeImplementationText"))
        assertEquals("Material Symbols Rounded", presentation.stringAt("iconFamily"))
        assertEquals(1, presentation.intAt("iconFill"))
        assertTrue(presentation.booleanAt("sharedIconCatalog"))
        assertTrue(presentation.booleanAt("unchangedStatusRefreshPreservesSessionList"))
        assertTrue(presentation.booleanAt("hoveredSelectionTargetSurvivesStatusRefresh"))
        assertTrue(presentation.booleanAt("pointerSequenceSurvivesStatusRefresh"))
        assertTrue(presentation.booleanAt("queueOwnsOverflow"))
        assertTrue(presentation.booleanAt("primaryActionRemainsReachable"))
        val preview = presentation.objectAt("previewMemory")
        assertFalse(preview.booleanAt("surfaceRetainsOriginalImageBytes"))
        assertFalse(preview.booleanAt("decodeBlocksInteractionThread"))
        assertFalse(preview.booleanAt("offscreenPreviewsHydrate"))
        assertTrue(preview.booleanAt("selectedPreviewHasPriority"))
        assertTrue(preview.booleanAt("backgroundHydrationYieldsToInteraction"))
        assertFalse(preview.booleanAt("queueRowsDecodeOriginalArtwork"))
        assertFalse(preview.booleanAt("historyRowsDecodeOriginalArtwork"))
        assertEquals(1_600, preview.intAt("stageMaximumEdgePixels"))
        assertEquals(128, preview.intAt("projectThumbnailMaximumEdgePixels"))
        assertEquals(12_288, preview.intAt("projectThumbnailMaximumEncodedBytes"))
        assertTrue(preview.booleanAt("projectThumbnailPersistedWithHistory"))
        assertEquals(2, preview.intAt("legacyProjectThumbnailDecodeLanes"))
        val projectPerformance = presentation.objectAt("projectPerformance")
        assertTrue(projectPerformance.booleanAt("stableRowsAcrossUnchangedRefresh"))
        assertTrue(projectPerformance.booleanAt("thumbnailCompletionPreservesInteractiveTargets"))
        assertTrue(segmentation.booleanAt("requireRenderableNormals"))
        assertTrue(segmentation.booleanAt("expandDisconnectedComponentsForSingleMesh"))
        assertTrue(segmentation.booleanAt("preserveExistingPartNodes"))
        assertEquals(24, segmentation.intAt("continuationWindowHours"))
        assertTrue(segmentation.booleanAt("newGenerationMayInvalidatePriorContinuation"))
        assertTrue(segmentation.booleanAt("automaticSeparationRunsAfterBaseCommit"))
        assertTrue(segmentation.booleanAt("automaticSeparationPreservesBaseWhileRunning"))
        assertTrue(segmentation.booleanAt("automaticSeparationFailurePreservesBase"))
        assertTrue(segmentation.booleanAt("automaticSeparationIsAtMostOncePerContinuation"))
        assertFalse(segmentation.booleanAt("quadInputSupported"))
        assertEquals("triangle", segmentation.stringAt("automaticSeparationBaseTopology"))
        assertEquals("quad", segmentation.stringAt("nonSeparatedQualityBaseTopology"))
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
        assertEquals("glb", quadCompanion.stringAt("previewFormat"))
        assertEquals("fbx", quadCompanion.stringAt("sourceFormat"))
        assertTrue(quadCompanion.booleanAt("publishedBesidePreview"))
        assertTrue(quadCompanion.booleanAt("savedResultNamesBothArtifacts"))
        assertTrue(quadCompanion.booleanAt("renameAndDeleteAreTransactionalForBothArtifacts"))
        assertTrue(quadCompanion.booleanAt("missingCompanionFailsPublication"))
        assertTrue(history.booleanAt("retainCurrentSessionResultUntilHistoryPersists"))
        assertTrue(history.booleanAt("doneStatusTransitionsDirectlyToModel"))
        assertTrue(history.booleanAt("freezeGenerationMode"))
        assertTrue(history.booleanAt("terminalItemCanBeReconfiguredAndRerun"))
        assertTrue(history.booleanAt("rerunPreservesPreviousOutput"))
        assertTrue(modelSafety.booleanAt("bufferByteLengthIsExactLogicalBytes"))
        assertTrue(modelSafety.booleanAt("binaryChunkUsesZeroAlignmentPadding"))
        assertTrue(modelSafety.booleanAt("binaryChunkMustBackBufferZero"))
        assertTrue(modelSafety.booleanAt("rendererBinaryFloatValuesMustBeFinite"))
        assertTrue(modelSafety.booleanAt("texturePayloadMustDecode"))
        assertTrue(modelSafety.booleanAt("sceneRootsUniqueAcrossScenes"))
        assertEquals(3, modelSafety.intAt("maximumBinaryAlignmentPaddingBytes"))
        assertEquals(1, runtimeCapabilities.intAt("contractVersion"))
        assertTrue(runtimeCapabilities.booleanAt("strictProductOnlyManifest"))
        assertTrue(runtimeCapabilities.booleanAt("malformedOrMissingCapabilityFailsClosed"))
        assertEquals(
            CreationContract.MAXIMUM_OPTIONAL_INSTRUCTION_CHARACTERS,
            optionalInstruction.intAt("maximumCharacters"),
        )
        assertEquals("none", presentation.stringAt("progressPreview"))
        assertTrue(presentation.booleanAt("wireframeAndOutlineIndependent"))
        val viewerControls = presentation.getValue("viewerControls").jsonArray
            .map { it.jsonPrimitive.content }
            .toSet()
        assertTrue(
            viewerControls.containsAll(
                setOf("orbit", "zoom", "pan", "grid", "wireframe", "auto_rotate", "toon", "outline"),
            ),
        )
    }

    @Test
    fun `image to svg product contract preserves canonical limits`() {
        val fixture = loadFixture("parity-fixtures/image-to-svg/state-contract.json")
        val limits = fixture.objectAt("limits")
        val input = fixture.objectAt("input")
        val submission = fixture.objectAt("submission")
        val recovery = fixture.objectAt("recovery")
        val models = fixture.objectAt("models")
        val transparentBackground = fixture.objectAt("transparentBackground")
        val backgroundModes = transparentBackground.objectAt("modes")
        val preview = fixture.objectAt("previewMemory")
        val presentation = fixture.objectAt("presentation")
        val viewer = fixture.objectAt("viewer")
        val pathSelection = viewer.objectAt("pathSelection")

        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.intAt("maximumParallelJobs"))
        assertEquals(1, input.intAt("minimumImagesPerJob"))
        assertEquals(1, input.intAt("maximumImagesPerJob"))
        assertTrue(input.booleanAt("multiplePickerImagesCreateIndependentSessions"))
        assertInputImageContract(input)
        assertEquals("selected_session", submission.stringAt("primaryActionScope"))
        assertFalse(submission.booleanAt("submitsOtherSessions"))
        assertTrue(submission.booleanAt("explicitSubmissionCreatesFreshDispatchId"))
        assertTrue(recovery.booleanAt("replayMatchesOnlySameDispatchId"))
        assertTrue(recovery.booleanAt("preparationRetriesAreBounded"))
        assertTrue(recovery.booleanAt("retryUsesFreshExecutionState"))
        assertTrue(recovery.booleanAt("uncleanWorkspaceIsQuarantined"))
        assertTrue(recovery.booleanAt("transientCapacityFailureIsCapabilityScoped"))
        assertTrue(recovery.booleanAt("temporaryCapacityPauseWaitIsBounded"))
        assertTrue(recovery.booleanAt("recoveryStorageCannotPermanentlyBlockPreparation"))
        assertTrue(recovery.booleanAt("inactivePreparationStateReclaimedBeforeAdmission"))
        assertTrue(recovery.booleanAt("liveAndAcceptedRecoveryStateProtected"))
        assertEquals(setOf("simple", "detail"), models.keys)
        assertTrue(models.objectAt("simple").booleanAt("selectable"))
        assertTrue(models.objectAt("detail").booleanAt("selectable"))
        assertEquals("opaque", transparentBackground.stringAt("default"))
        assertEquals(setOf("auto", "transparent", "opaque"), backgroundModes.keys)
        assertTrue(transparentBackground.booleanAt("capturedPerSubmission"))
        assertTrue(transparentBackground.booleanAt("preservedThroughRecovery"))
        assertTrue(transparentBackground.booleanAt("recordedInHistory"))
        assertFalse(preview.booleanAt("surfaceRetainsOriginalImageBytes"))
        assertFalse(preview.booleanAt("decodeBlocksInteractionThread"))
        assertFalse(preview.booleanAt("queueRowsDecodeArtwork"))
        assertFalse(preview.booleanAt("historyRowsDecodeArtwork"))
        assertFalse(preview.booleanAt("sourceSettingDecodesArtwork"))
        assertEquals(1, preview.intAt("maximumSelectedRasterPreviews"))
        assertTrue(preview.booleanAt("selectedPreviewUsesPlatformImageDecoder"))
        assertFalse(preview.booleanAt("selectedPreviewTransformsOnInteractionThread"))
        assertFalse(preview.booleanAt("persistentPreviewCache"))
        assertEquals("Material Symbols Rounded", presentation.stringAt("iconFamily"))
        assertEquals(1, presentation.intAt("iconFill"))
        assertTrue(presentation.booleanAt("sharedIconCatalog"))
        assertTrue(presentation.booleanAt("unchangedStatusRefreshPreservesSessionList"))
        assertTrue(presentation.booleanAt("hoveredSelectionTargetSurvivesStatusRefresh"))
        assertTrue(presentation.booleanAt("pointerSequenceSurvivesStatusRefresh"))
        assertTrue(presentation.booleanAt("queueOwnsOverflow"))
        assertTrue(presentation.booleanAt("primaryActionRemainsReachable"))
        assertEquals("none", presentation.stringAt("progressPreview"))
        assertTrue(viewer.booleanAt("completeSafeStaticPresentationRendered"))
        assertTrue(viewer.booleanAt("rejectXmlProcessingInstructions"))
        assertTrue(viewer.booleanAt("requireCanonicalSvgNamespace"))
        assertTrue(viewer.booleanAt("allowEmbeddedRasterOnlyOnImageHref"))
        assertEquals("explicit", viewer.stringAt("editableSurfaceActivation"))
        assertFalse(viewer.booleanAt("selectionBuildsEditableSurface"))
        assertFalse(viewer.booleanAt("selectionTransfersEditableDocument"))
        assertFalse(viewer.booleanAt("staticPresentationAnimatesIndividualPaths"))
        assertTrue(pathSelection.booleanAt("stationaryPrimaryPressSelectsGeometry"))
        assertTrue(pathSelection.booleanAt("pointerCaptureBeginsAfterPanThreshold"))
        assertTrue(pathSelection.booleanAt("captureCannotRetargetSelection"))
        assertTrue(pathSelection.booleanAt("panDoesNotChangeSelection"))
        assertEquals(
            CreationContract.MAXIMUM_EDITABLE_SVG_BYTES,
            viewer.longAt("maximumEditableDocumentBytes"),
        )
        assertEquals(
            CreationContract.MAXIMUM_EDITABLE_SVG_GEOMETRY,
            viewer.intAt("maximumEditableGeometryElements"),
        )
        assertEquals(
            CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES,
            viewer.longAt("maximumDocumentBytes"),
        )
        assertEquals(
            CreationContract.MAXIMUM_SVG_ELEMENTS,
            viewer.intAt("maximumDocumentElements"),
        )
        assertEquals(
            CreationContract.MAXIMUM_SVG_ELEMENTS,
            viewer.intAt("maximumExpandedElementOccurrences"),
        )
        assertEquals(
            CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_EDGES,
            viewer.intAt("maximumExpandedReferenceOccurrences"),
        )
        assertEquals(
            CreationContract.MAXIMUM_SVG_ATTRIBUTES,
            viewer.intAt("maximumDocumentAttributes"),
        )
        assertEquals(
            CreationContract.MAXIMUM_SVG_EMBEDDED_RASTER_CHARACTERS,
            viewer.intAt("maximumEmbeddedRasterCharacters"),
        )
        assertEquals(
            CreationContract.MAXIMUM_SVG_EMBEDDED_RASTER_PIXELS,
            viewer.longAt("maximumEmbeddedRasterPixels"),
        )
        assertEquals(
            CreationContract.MAXIMUM_SVG_TOTAL_EMBEDDED_RASTER_PIXELS,
            viewer.longAt("maximumTotalEmbeddedRasterPixels"),
        )
        assertTrue(viewer.booleanAt("totalEmbeddedRasterPixelsCountEveryOccurrence"))
        assertTrue(viewer.booleanAt("localReferenceExpansionPreservesMultiplicity"))
        assertTrue(viewer.booleanAt("localReferenceExpansionChargesReferencedRasterOccurrences"))
        assertEquals(
            setOf("image/png", "image/jpeg"),
            viewer.getValue("allowedEmbeddedRasterMimeTypes").jsonArray
                .map { it.jsonPrimitive.content }
                .toSet(),
        )
        assertTrue(viewer.booleanAt("boundedUndoMemory"))
        assertTrue(viewer.booleanAt("undoUsesDeltasOrCheckpoints"))
    }

    @Test
    fun `unrecognized stages cannot expand product progress`() {
        assertEquals(
            "waiting_for_user",
            publicCreationStage(CreationTool.IMAGE_TO_3D, "waiting_for_user", "preparing"),
        )
        assertEquals(
            "Complete the requested step in the secure window",
            publicCreationProgressText("waiting_for_user"),
        )
        assertEquals(
            "preparing",
            publicCreationStage(CreationTool.IMAGE_TO_3D, "uploading", "preparing"),
        )
        assertEquals(
            "generating",
            publicCreationStage(CreationTool.IMAGE_TO_3D, "unrecognized-stage", "generating"),
        )
        assertEquals(
            "preparing",
            publicCreationStage(CreationTool.IMAGE_TO_SVG, "segmenting", "preparing"),
        )
        assertEquals(
            "finalizing",
            publicCreationStage(CreationTool.IMAGE_TO_SVG, "unrecognized-stage", "finalizing"),
        )
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
    private fun JsonObject.longAt(key: String) = requireNotNull(this[key]).jsonPrimitive.long
    private fun JsonObject.doubleAt(key: String) = requireNotNull(this[key]).jsonPrimitive.double
    private fun JsonObject.stringAt(key: String) = requireNotNull(this[key]).jsonPrimitive.content
    private fun JsonObject.booleanAt(key: String) = requireNotNull(this[key]).jsonPrimitive.boolean

    private fun assertInputImageContract(input: JsonObject) {
        assertEquals(
            CreationContract.MAXIMUM_PICKER_BATCH_IMAGES,
            input.intAt("maximumPickerBatchImages"),
        )
        assertEquals(
            CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES,
            input.longAt("maximumBytesPerImage"),
        )
        assertEquals(
            CreationContract.MAXIMUM_PICKER_AGGREGATE_BYTES,
            input.longAt("maximumPickerAggregateBytes"),
        )
        assertEquals(
            CreationContract.MAXIMUM_IMAGE_DIMENSION,
            input.intAt("maximumDimensionPixels"),
        )
        assertEquals(
            CreationContract.MAXIMUM_DECODED_IMAGE_PIXELS,
            input.longAt("maximumDecodedPixelsPerImage"),
        )
        assertEquals(
            setOf("image/png", "image/jpeg", "image/webp"),
            input.getValue("supportedMimeTypes").jsonArray
                .map { it.jsonPrimitive.content }
                .toSet(),
        )
    }
}
