package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import java.io.File
import kotlinx.serialization.json.Json
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

class CreationParityContractTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `image to 3d Android contract matches shared fixture`() {
        val fixture = fixture("image-to-3d")
        val limits = fixture["limits"]!!.jsonObject
        val defaults = fixture["defaults"]!!.jsonObject
        val modelSafety = fixture["modelSafety"]!!.jsonObject
        val names = fixture["names"]!!.jsonObject
        val readiness = fixture["readiness"]!!.jsonObject
        val presentation = fixture["presentation"]!!.jsonObject
        val lifecycle = fixture["hostLifecycle"]!!.jsonObject
        val recovery = fixture["recovery"]!!.jsonObject
        val distribution = fixture["distribution"]!!.jsonObject
        val qualityControl = fixture["qualityControl"]!!.jsonObject

        assertEquals(CreationContract.MINIMUM_POLYCOUNT, limits.int("minimumPolycount"))
        assertEquals(CreationContract.MAXIMUM_POLYCOUNT, limits.int("maximumPolycount"))
        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.int("maximumParallelJobs"))
        assertEquals(CreationContract.DEFAULT_POLYCOUNT, defaults.int("polycount"))
        assertTrue(modelSafety.boolean("staticTriangleGeometryOnly"))
        assertEquals(
            CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES,
            modelSafety.int("maximumGlbBytes").toLong(),
        )
        assertEquals(CREATION_GLB_MAXIMUM_JSON_BYTES, modelSafety.int("maximumJsonBytes"))
        assertEquals(
            CREATION_GLB_MAXIMUM_DATA_URI_CHARACTERS,
            modelSafety.int("maximumEmbeddedUriCharacters"),
        )
        assertEquals(CREATION_GLB_MAXIMUM_BUFFERS, modelSafety.int("maximumBuffers"))
        assertEquals(
            CREATION_GLB_MAXIMUM_BUFFER_VIEWS,
            modelSafety.int("maximumBufferViews"),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_AGGREGATE_BUFFER_VIEW_BYTES,
            modelSafety.int("maximumAggregateBufferViewBytes").toLong(),
        )
        assertEquals(CREATION_GLB_MAXIMUM_ACCESSORS, modelSafety.int("maximumAccessors"))
        assertEquals(
            CREATION_GLB_MAXIMUM_ACCESSOR_ELEMENTS,
            modelSafety.int("maximumAccessorElements").toLong(),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE,
            modelSafety.double("maximumAbsoluteRendererValue"),
            0.0,
        )
        assertEquals(
            CREATION_GLB_POSITION_BOUNDS_ABSOLUTE_TOLERANCE,
            modelSafety.double("maximumPositionBoundsAbsoluteTolerance"),
            0.0,
        )
        assertEquals(
            CREATION_GLB_POSITION_BOUNDS_RELATIVE_TOLERANCE,
            modelSafety.double("maximumPositionBoundsRelativeTolerance"),
            0.0,
        )
        assertEquals(CREATION_GLB_MAXIMUM_NODES, modelSafety.int("maximumNodes"))
        assertEquals(CREATION_GLB_MAXIMUM_SCENES, modelSafety.int("maximumScenes"))
        assertEquals(CREATION_GLB_MAXIMUM_MESHES, modelSafety.int("maximumMeshes"))
        assertEquals(CREATION_GLB_MAXIMUM_PRIMITIVES, modelSafety.int("maximumPrimitives"))
        assertEquals(CREATION_GLB_MAXIMUM_MATERIALS, modelSafety.int("maximumMaterials"))
        assertEquals(
            CREATION_GLB_MAXIMUM_VERTICES,
            modelSafety.int("maximumVertices").toLong(),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_INDICES,
            modelSafety.int("maximumIndices").toLong(),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_MORPH_TARGETS,
            modelSafety.int("maximumMorphTargets"),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_MORPH_ELEMENTS,
            modelSafety.int("maximumMorphElements").toLong(),
        )
        assertEquals(CREATION_GLB_MAXIMUM_SKINS, modelSafety.int("maximumSkins"))
        assertEquals(
            CREATION_GLB_MAXIMUM_JOINTS_PER_SKIN,
            modelSafety.int("maximumJointsPerSkin"),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_TOTAL_JOINTS,
            modelSafety.int("maximumTotalJoints"),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_PRIMITIVE_ATTRIBUTES,
            modelSafety.int("maximumPrimitiveAttributes"),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_MORPH_ATTRIBUTES,
            modelSafety.int("maximumMorphAttributes"),
        )
        assertEquals(CREATION_GLB_MAXIMUM_IMAGES, modelSafety.int("maximumImages"))
        assertEquals(CREATION_GLB_MAXIMUM_TEXTURES, modelSafety.int("maximumTextures"))
        assertEquals(CREATION_GLB_MAXIMUM_SAMPLERS, modelSafety.int("maximumSamplers"))
        assertEquals(
            CREATION_GLB_MAXIMUM_IMAGE_DIMENSION,
            modelSafety.int("maximumTextureAxisPixels"),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_IMAGE_PIXELS,
            modelSafety.int("maximumPixelsPerTextureImage").toLong(),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_DECODED_IMAGE_PIXELS,
            modelSafety.int("maximumDecodedImagePixels").toLong(),
        )
        assertEquals(
            CREATION_GLB_MAXIMUM_REFERENCED_TEXTURE_PIXELS,
            modelSafety.int("maximumReferencedTexturePixels").toLong(),
        )
        assertTrue(modelSafety.boolean("bufferByteLengthIsExactLogicalBytes"))
        assertTrue(modelSafety.boolean("binaryChunkUsesZeroAlignmentPadding"))
        assertTrue(modelSafety.boolean("binaryChunkMustBackBufferZero"))
        assertEquals(3, modelSafety.int("maximumBinaryAlignmentPaddingBytes"))
        assertEquals("2.0", modelSafety.string("exactAssetVersion"))
        assertEquals(32, modelSafety.int("jsonChunkPaddingByte"))
        assertEquals(CREATION_GLB_MAXIMUM_NODE_DEPTH, modelSafety.int("maximumNodeDepth"))
        listOf(
            "accessorAbsoluteAlignmentRequired",
            "vertexAccessorFourByteAlignmentRequired",
            "loaderInterleavedTailCoverageRequired",
            "accessorBoundsValidated",
            "positionBoundsContainBinaryValues",
            "rendererBinaryFloatValuesMustBeFinite",
            "primitiveElementCountMultipleOfThree",
            "primitiveIndicesWithinPositionAccessor",
            "sharedIndexedVertexStorageChargedOnce",
            "texturePayloadMustDecode",
            "materialTextureReferencesValidated",
            "materialNumericValuesBounded",
            "materialRendererValueTypesValidated",
            "textureClonePixelsCharged",
            "textureTransformValuesBounded",
            "samplerEnumsValidated",
            "bufferUriMimeContextRequired",
            "presentationRevalidatesCommittedBytesBeforeLoad",
            "selectedSceneMustContainGeometry",
            "sceneRootsUniqueAcrossScenes",
            "nodeTransformsAndMorphWeightsBounded",
            "skinsAllowed",
            "skinReferencesValidated",
            "inverseBindMatricesValidated",
            "jointIndicesWithinSkin",
            "skinWeightsNormalized",
            "skinScopesUnambiguous",
            "extensionBodiesMustBeObjects",
        ).forEach { field -> assertTrue(field, modelSafety.boolean(field)) }
        assertFalse(modelSafety.boolean("externalResourcesAllowed"))
        assertEquals(
            setOf("image/png", "image/jpeg", "image/webp"),
            modelSafety.getValue("embeddedImageMimeTypes").jsonArray
                .map { it.jsonPrimitive.content }
                .toSet(),
        )
        assertFalse(modelSafety.boolean("animatedPngAllowed"))
        assertFalse(modelSafety.boolean("animatedWebpAllowed"))
        assertFalse(modelSafety.boolean("sparseAccessorsAllowed"))
        assertFalse(modelSafety.boolean("animationsAllowed"))
        assertFalse(modelSafety.boolean("authoredCamerasAllowed"))
        assertEquals("acyclic_single_parent", modelSafety.string("nodeGraphPolicy"))
        assertTrue(modelSafety.boolean("extensionsFailClosed"))
        assertTrue(modelSafety.boolean("extensionsUsedMustBeUnique"))
        assertTrue(modelSafety.boolean("extensionsRequiredMustBeUsed"))
        assertTrue(modelSafety.boolean("extensionBodiesMustBeDeclared"))
        assertEquals(
            CREATION_GLB_ALLOWED_EXTENSIONS,
            modelSafety.getValue("allowedExtensions").jsonArray
                .map { it.jsonPrimitive.content }
                .toSet(),
        )
        assertTrue(presentation.boolean("sharedIconCatalog"))
        assertTrue(readiness.boolean("unresponsiveOperationIsBoundedBeforeWholeJobWatchdog"))
        assertTrue(readiness.boolean("uncertainConsequentialActionIsNotRepeated"))
        assertTrue(presentation.boolean("unchangedStatusRefreshPreservesSessionList"))
        assertTrue(presentation.boolean("hoveredSelectionTargetSurvivesStatusRefresh"))
        val geometryStats = presentation.getValue("geometryStats").jsonObject
        assertTrue(geometryStats.boolean("sharedVertexStorageCountedOnce"))
        assertTrue(geometryStats.boolean("distinctIndexedPrimitivesCountFacesOnce"))
        assertFalse(geometryStats.boolean("repeatedPresentationInstancesInflateTopology"))
        assertTrue(lifecycle.boolean("closeCancelsOnlyOwnerJobs"))
        assertTrue(lifecycle.boolean("closeDestroysProductSurface"))
        assertTrue(lifecycle.boolean("executionLossRequeuesActiveJobExactlyOnce"))
        assertTrue(recovery.boolean("acceptedExecutionLossRetainsDispatchAndInput"))
        assertTrue(recovery.boolean("acceptedExecutionLossRequeuesSameDispatch"))
        assertTrue(recovery.boolean("acceptedExecutionLossIsNotTerminalFailure"))
        assertTrue(distribution.boolean("behaviorIdentical"))
        assertTrue(distribution.boolean("featureSetIdentical"))
        assertRealUiQualityControl(qualityControl, setOf("quality"))
        assertEquals(names.string("en"), MobileLocaleText.forLanguage("en").appImageTo3dTitle)
        assertEquals(names.string("ko"), MobileLocaleText.forLanguage("ko").appImageTo3dTitle)
        assertEquals(names.string("vi"), MobileLocaleText.forLanguage("vi").appImageTo3dTitle)
    }

    @Test
    fun `image to SVG Android contract matches shared fixture`() {
        val fixture = fixture("image-to-svg")
        val availability = fixture["releaseAvailability"]!!.jsonObject
        val limits = fixture["limits"]!!.jsonObject
        val models = fixture["models"]!!.jsonObject
        val viewer = fixture["viewer"]!!.jsonObject
        val pathSelection = viewer["pathSelection"]!!.jsonObject
        val presentation = fixture["presentation"]!!.jsonObject
        val lifecycle = fixture["hostLifecycle"]!!.jsonObject
        val recovery = fixture["recovery"]!!.jsonObject
        val distribution = fixture["distribution"]!!.jsonObject
        val qualityControl = fixture["qualityControl"]!!.jsonObject

        assertTrue(availability["enabled"]!!.jsonPrimitive.boolean)
        assertTrue(availability["entryVisible"]!!.jsonPrimitive.boolean)
        assertEquals(
            "open_surface",
            availability["entryBehavior"]!!.jsonPrimitive.content,
        )
        assertTrue(availability["startsSurface"]!!.jsonPrimitive.boolean)
        assertTrue(availability["startsReadiness"]!!.jsonPrimitive.boolean)
        assertTrue(availability["preservesPreparedCapacity"]!!.jsonPrimitive.boolean)
        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.int("maximumParallelJobs"))
        assertEquals(setOf("simple", "detail"), models.keys)
        assertTrue(models.getValue("simple").jsonObject.boolean("selectable"))
        assertTrue(models.getValue("detail").jsonObject.boolean("selectable"))
        assertEquals(
            CREATION_SVG_MAXIMUM_DOCUMENT_DEPTH,
            viewer.int("maximumDocumentDepth"),
        )
        assertEquals(
            CREATION_SVG_MAXIMUM_PATH_COMMANDS,
            viewer.int("maximumPathCommands"),
        )
        assertEquals(
            CREATION_SVG_MAXIMUM_GEOMETRY_NUMBERS,
            viewer.int("maximumGeometryNumbers"),
        )
        assertEquals(
            CREATION_SVG_MAXIMUM_COORDINATE_MAGNITUDE,
            viewer.double("maximumCoordinateMagnitude"),
            0.0,
        )
        assertEquals(
            CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_EDGES,
            viewer.int("maximumLocalReferenceEdges"),
        )
        assertEquals(
            CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_DEPTH,
            viewer.int("maximumLocalReferenceDepth"),
        )
        assertEquals(
            CREATION_SVG_MAXIMUM_LOCAL_IDENTIFIER_BYTES,
            viewer.int("maximumLocalIdentifierBytes"),
        )
        assertTrue(viewer.boolean("rejectDuplicateLocalIdentifiers"))
        assertTrue(viewer.boolean("rejectCyclicLocalReferences"))
        assertTrue(viewer.boolean("rejectEncodedLocalReferenceAliases"))
        assertFalse(viewer.boolean("allowStylesheetLocalReferences"))
        assertTrue(viewer.boolean("rejectCssMotion"))
        assertTrue(pathSelection.boolean("stationaryPrimaryPressSelectsGeometry"))
        assertTrue(pathSelection.boolean("pointerCaptureBeginsAfterPanThreshold"))
        assertTrue(pathSelection.boolean("captureCannotRetargetSelection"))
        assertTrue(pathSelection.boolean("panDoesNotChangeSelection"))
        assertTrue(presentation.boolean("sharedIconCatalog"))
        assertTrue(presentation.boolean("unchangedStatusRefreshPreservesSessionList"))
        assertTrue(presentation.boolean("hoveredSelectionTargetSurvivesStatusRefresh"))
        assertTrue(lifecycle.boolean("closeCancelsOnlyOwnerJobs"))
        assertTrue(lifecycle.boolean("closeDestroysProductSurface"))
        assertTrue(recovery.boolean("preparationRetriesAreBounded"))
        assertTrue(recovery.boolean("retryUsesFreshExecutionState"))
        assertTrue(recovery.boolean("uncleanWorkspaceIsQuarantined"))
        assertTrue(recovery.boolean("transientCapacityFailureIsCapabilityScoped"))
        assertTrue(recovery.boolean("temporaryCapacityPauseWaitIsBounded"))
        assertTrue(recovery.boolean("recoveryStorageCannotPermanentlyBlockPreparation"))
        assertTrue(recovery.boolean("inactivePreparationStateReclaimedBeforeAdmission"))
        assertTrue(recovery.boolean("liveAndAcceptedRecoveryStateProtected"))
        assertTrue(distribution.boolean("behaviorIdentical"))
        assertTrue(distribution.boolean("featureSetIdentical"))
        assertRealUiQualityControl(qualityControl, setOf("simple"))
    }

    @Test
    fun `image creator Android contract matches shared fixture`() {
        val fixture = fixture("image-creation-editing")
        val availability = fixture["releaseAvailability"]!!.jsonObject
        val prompt = fixture["request"]!!.jsonObject["prompt"]!!.jsonObject
        val references = fixture["request"]!!.jsonObject["references"]!!.jsonObject
        val locales = fixture["locales"]!!.jsonObject
        assertFalse(availability["enabled"]!!.jsonPrimitive.boolean)
        assertFalse(availability["entryVisible"]!!.jsonPrimitive.boolean)
        assertEquals(
            "hidden",
            availability["entryBehavior"]!!.jsonPrimitive.content,
        )
        assertFalse(availability["startsSurface"]!!.jsonPrimitive.boolean)
        assertFalse(availability["startsReadiness"]!!.jsonPrimitive.boolean)
        assertTrue(availability["preservesPreparedCapacity"]!!.jsonPrimitive.boolean)
        val presentation = fixture["presentation"]!!.jsonObject
        val behavior = fixture["behavior"]!!.jsonObject
        val readiness = fixture["readiness"]!!.jsonObject
        val recovery = fixture["recovery"]!!.jsonObject
        val distribution = fixture["distribution"]!!.jsonObject
        val qualityControl = fixture["qualityControl"]!!.jsonObject
        val copyPolicy = fixture["publicCopyPolicy"]!!.jsonObject

        assertEquals("image", fixture.string("tool"))
        assertEquals(CreationContract.IMAGE_CREATOR_OPERATION, fixture.string("operation"))
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_PARALLEL_JOBS,
            fixture.int("maximumParallelJobs"),
        )
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_CONCURRENT_PREPARATIONS,
            readiness.int("maximumConcurrentFreshPreparations"),
        )
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_PROMPT_CHARACTERS,
            prompt.int("maximumCharacters"),
        )
        assertEquals(0, references.int("minimum"))
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES,
            references.int("maximum"),
        )
        assertEquals("Google Sans Flex", presentation.string("fontFamily"))
        assertEquals(100, presentation.int("fontRoundedAxis"))
        assertEquals("Material Symbols Rounded", presentation.string("iconFamily"))
        assertEquals(1, presentation.int("iconFill"))
        assertFalse(presentation.boolean("appSpecificTheme"))
        assertTrue(presentation.boolean("sharedIconCatalog"))
        assertTrue(presentation.boolean("matchesSharedCreationExperience"))
        assertTrue(presentation.boolean("unchangedStatusRefreshPreservesSessionList"))
        assertTrue(presentation.boolean("hoveredSelectionTargetSurvivesStatusRefresh"))
        assertTrue(presentation.boolean("focusedInputSurvivesStatusRefresh"))
        assertTrue(presentation.boolean("imeCompositionSurvivesStatusRefresh"))
        val estimatedProgress = presentation["estimatedProgress"]!!.jsonObject
        assertTrue(estimatedProgress.boolean("usesRuntimeEstimate"))
        assertTrue(estimatedProgress.boolean("usesElapsedTimeCurve"))
        assertTrue(estimatedProgress.boolean("monotonic"))
        assertEquals(0.94, estimatedProgress.double("maximumBeforeCompletion"), 0.0)
        assertTrue(estimatedProgress.boolean("showsLocalizedEta"))
        assertEquals("feature_only", copyPolicy.string("vocabulary"))
        assertFalse(copyPolicy.boolean("implementationDetailsVisible"))
        assertFalse(copyPolicy.boolean("rawImplementationErrorsVisible"))
        assertTrue(copyPolicy.boolean("referenceUploadCopyRequiresReferences"))
        assertTrue(behavior.boolean("cancellationIsMonotonic"))
        assertTrue(behavior.boolean("lateSuccessCannotPublishAfterCancellation"))
        assertTrue(behavior.boolean("retryCreatesNewJob"))
        assertTrue(behavior.boolean("retryPreservesPreviousResult"))
        assertTrue(behavior.boolean("closeCancelsOnlyOwnerJobs"))
        assertTrue(behavior.boolean("closeReleasesOwnerExecutionResources"))
        assertTrue(behavior.boolean("closeDestroysProductSurface"))
        assertTrue(behavior.boolean("failureRemainsBoundToJob"))
        assertTrue(readiness.boolean("usesCanonicalVisibleEntrySequence"))
        assertTrue(readiness.boolean("intermediateNavigationIsNotReadiness"))
        assertTrue(readiness.boolean("authenticatedWorkspaceRequired"))
        assertTrue(recovery.boolean("acceptedRequestResumedWithoutResubmit"))
        assertTrue(recovery.boolean("workerCleanupPrecedesRecoveryRedispatch"))
        assertTrue(recovery.boolean("replayMatchesOnlySameDispatchId"))
        assertTrue(distribution.boolean("behaviorIdentical"))
        assertTrue(distribution.boolean("featureSetIdentical"))
        assertRealUiQualityControl(qualityControl, emptySet())
        assertTrue(qualityControl.boolean("consequentialTransitionUsesRealControlInteraction"))
        assertTrue(qualityControl.boolean("intermediateNavigationIsNotReadinessProof"))
        assertTrue(qualityControl.boolean("transientPreparationRetryIsBounded"))
        assertTrue(qualityControl.boolean("transientPreparationRetryUsesFreshCapacity"))
        assertTrue(qualityControl.boolean("acceptedRequestIsNotRepeatedByPreparationRetry"))
        assertEquals(locales.string("en"), MobileLocaleText.forLanguage("en").appImageCreatorTitle)
        assertEquals(locales.string("ko"), MobileLocaleText.forLanguage("ko").appImageCreatorTitle)
        assertEquals(locales.string("vi"), MobileLocaleText.forLanguage("vi").appImageCreatorTitle)
    }

    private fun fixture(tool: String) = json.parseToJsonElement(
        File(repoRoot(), "parity-fixtures/$tool/state-contract.json").readText(),
    ).jsonObject

    private fun assertRealUiQualityControl(
        contract: kotlinx.serialization.json.JsonObject,
        cases: Set<String>,
    ) {
        assertEquals(
            setOf("windows", "android_full", "android_play"),
            contract.getValue("realUiPlatforms").jsonArray
                .map { it.jsonPrimitive.content }
                .toSet(),
        )
        assertEquals(
            cases,
            contract.getValue("cases").jsonArray.map { it.jsonPrimitive.content }.toSet(),
        )
        assertTrue(contract.boolean("terminalStateRequired"))
        assertTrue(contract.boolean("committedArtifactValidated"))
        assertTrue(contract.boolean("terminalFailureEvidenceRequired"))
    }

    private fun repoRoot(): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = directory.parentFile?.canonicalFile ?: error("Could not find repository root")
        }
        return directory
    }
}

private fun kotlinx.serialization.json.JsonObject.int(key: String): Int =
    this[key]!!.jsonPrimitive.int

private fun kotlinx.serialization.json.JsonObject.string(key: String): String =
    this[key]!!.jsonPrimitive.content

private fun kotlinx.serialization.json.JsonObject.boolean(key: String): Boolean =
    this[key]!!.jsonPrimitive.boolean

private fun kotlinx.serialization.json.JsonObject.double(key: String): Double =
    this[key]!!.jsonPrimitive.double
