package dev.screengoated.toolbox.mobile.phonecontrol

import java.io.File

internal fun phoneControlRepoRoot(requiredPath: String): File {
    val workingDirectory = requireNotNull(System.getProperty("user.dir"))
    return generateSequence(File(workingDirectory).absoluteFile) { current ->
        current.parentFile ?: return@generateSequence null
    }.firstOrNull { root -> File(root, requiredPath).isFile }
        ?: error("Could not locate $requiredPath from $workingDirectory")
}
