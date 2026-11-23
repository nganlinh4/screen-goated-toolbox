package dev.screengoated.toolbox.mobile.creation

internal object CreationDeliveryTestFixture {
    fun prepared(request: CreationWorkerRequest = request()) = PreparedCreation(
        engineId = "engine",
        ownerId = "owner",
        request = request,
        current = CreationJobStatus(
            jobId = request.jobId,
            stage = "finalizing",
            progressText = "working",
        ),
        event = CreationWorkerEvent(
            jobId = request.jobId,
            event = "completed",
        ),
        stagingPath = "creation/staging/result.commit",
        mimeType = "model/gltf-binary",
        segmented = false,
        canSegment = true,
        faces = 10,
        vertices = 5,
    )

    fun record(prepared: PreparedCreation): CreationDeliveryRecord =
        CreationDeliveryRecord(
            dispatchId = prepared.request.dispatchId,
            engineId = prepared.engineId,
            ownerId = prepared.ownerId,
            request = prepared.request,
            current = prepared.current,
            event = prepared.event,
            sealedPath = prepared.stagingPath,
            mimeType = prepared.mimeType,
            segmented = prepared.segmented,
            canSegment = prepared.canSegment,
            faces = prepared.faces,
            vertices = prepared.vertices,
            artifactSize = 4,
            artifactSha256 = "a".repeat(64),
            intent = CreationPublishIntent(
                kind = "managed",
                finalName = "result.glb",
                mimeType = "model/gltf-binary",
            ),
        )

    fun request(
        jobId: String = "3d_job",
        dispatchId: String = "dispatch_3d_job",
    ): CreationWorkerRequest {
        val unsigned = CreationWorkerRequest(
            jobId = jobId,
            dispatchId = dispatchId,
            sourceDescriptors = listOf(
                CreationSourceDescriptor("creation/job-inputs/source.png", 4, "b".repeat(64)),
            ),
            tool = CreationTool.IMAGE_TO_3D.wireName,
            generationMode = CreationGenerationMode.QUALITY.wireName,
            operation = "generate",
            imagePath = "creation/job-inputs/source.png",
            outputPath = "creation/staging/result.glb",
            outputName = "result.glb",
        )
        return unsigned.copy(requestFingerprint = creationRequestFingerprint(unsigned))
    }
}
