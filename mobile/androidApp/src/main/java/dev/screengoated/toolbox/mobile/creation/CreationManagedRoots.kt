package dev.screengoated.toolbox.mobile.creation

import java.io.File

internal fun creationManagedArtifactRoots(filesDir: File, cacheDir: File): List<File> = listOf(
    File(filesDir, "creation/sources"),
    File(filesDir, "creation/job-inputs"),
    File(filesDir, "creation/library"),
    File(filesDir, "creation/presentation"),
    File(filesDir, "creation/relinquished"),
    File(filesDir, "creation/staging"),
    File(cacheDir, "creation/previews"),
)

internal fun isUserOwnedCreationOutputPath(path: String): Boolean = path.startsWith("content://")

internal fun creationCommittedProofPath(path: String, managedIdentity: String?): String? =
    if (isUserOwnedCreationOutputPath(path)) path else managedIdentity
