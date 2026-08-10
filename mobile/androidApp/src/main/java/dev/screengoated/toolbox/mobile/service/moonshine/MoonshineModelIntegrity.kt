package dev.screengoated.toolbox.mobile.service.moonshine

import java.io.File

internal object MoonshineModelIntegrity {
    fun payloadPresent(directory: File, files: List<MoonshineModelFile>): Boolean =
        ManagedModelIntegrity.payloadPresent(directory, files)

    fun verified(directory: File, files: List<MoonshineModelFile>): Boolean =
        ManagedModelIntegrity.verified(directory, files)

    fun verified(file: File, contract: MoonshineModelFile): Boolean =
        ManagedModelIntegrity.verified(file, contract)

    fun finalizeVerifiedPart(part: File, target: File, contract: MoonshineModelFile) =
        ManagedModelIntegrity.finalizeVerifiedPart(part, target, contract)

    fun removeManagedFiles(directory: File, files: List<MoonshineModelFile>): Boolean =
        ManagedModelIntegrity.removeManagedFiles(directory, files)
}
