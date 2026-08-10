package dev.screengoated.toolbox.mobile.service.moonshine

import java.io.File

internal object ZipformerModelIntegrity {
    fun payloadPresent(directory: File, files: List<ZipformerModelFile>): Boolean =
        ManagedModelIntegrity.payloadPresent(directory, files)

    fun verified(directory: File, files: List<ZipformerModelFile>): Boolean =
        ManagedModelIntegrity.verified(directory, files)

    fun verified(file: File, contract: ZipformerModelFile): Boolean =
        ManagedModelIntegrity.verified(file, contract)

    fun finalizeVerifiedPart(part: File, target: File, contract: ZipformerModelFile) =
        ManagedModelIntegrity.finalizeVerifiedPart(part, target, contract)

    fun removeManagedFiles(directory: File, files: List<ZipformerModelFile>): Boolean =
        ManagedModelIntegrity.removeManagedFiles(directory, files)
}
