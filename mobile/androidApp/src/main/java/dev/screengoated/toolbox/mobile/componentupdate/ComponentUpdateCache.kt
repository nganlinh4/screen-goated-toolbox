package dev.screengoated.toolbox.mobile.componentupdate

import android.content.Context
import dev.screengoated.toolbox.mobile.BuildConfig
import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.StandardCopyOption

internal object ComponentUpdateRuntime {
    @Volatile private var applicationContext: Context? = null

    fun initialize(context: Context) {
        applicationContext = context.applicationContext
    }

    fun context(): Context = requireNotNull(applicationContext) {
        "Component update runtime is not initialized"
    }
}

internal object ComponentUpdateCache {
    fun loadHighest(context: Context): VerifiedComponentCatalog? {
        ComponentUpdateRuntime.initialize(context)
        val root = root(context)
        if (!root.exists()) return null
        requireRegularDirectory(root)
        val candidates = root.listFiles().orEmpty()
            .filter { CACHED_CATALOG.matches(it.name) }
            .take(MAXIMUM_CACHED_CATALOGS + 1)
        require(candidates.size <= MAXIMUM_CACHED_CATALOGS)
        return candidates.mapNotNull { catalogFile ->
            runCatching {
                val catalog = readRegular(catalogFile, MAXIMUM_CATALOG_BYTES.toLong())
                val signature = readRegular(
                    File(root, catalogFile.name.removeSuffix(".json") + ".sig"),
                    64L,
                )
                verifyComponentCatalog(context, catalog, signature, BuildConfig.VERSION_NAME)
            }.getOrNull()
        }.maxByOrNull(VerifiedComponentCatalog::sequence)
    }

    @Synchronized
    fun store(context: Context, name: String, catalog: ByteArray, signature: ByteArray) {
        require(CACHED_CATALOG.matches(name))
        val root = root(context).apply { mkdirs() }
        requireRegularDirectory(root)
        storeOne(root, name, catalog)
        storeOne(root, name.removeSuffix(".json") + ".sig", signature)
    }

    private fun storeOne(root: File, name: String, bytes: ByteArray) {
        val target = File(root, name)
        if (target.exists()) {
            require(readRegular(target, bytes.size.toLong()).contentEquals(bytes)) {
                "Existing component catalog cache entry has different bytes"
            }
            return
        }
        val temporary = File(root, "$name.${android.os.Process.myPid()}.download")
        if (temporary.exists()) {
            require(
                Files.isRegularFile(temporary.toPath(), LinkOption.NOFOLLOW_LINKS) &&
                    !Files.isSymbolicLink(temporary.toPath()),
            ) { "Component catalog staging path is unsafe" }
            check(temporary.delete()) { "Could not clear stale component catalog staging file" }
        }
        check(temporary.createNewFile()) { "Component catalog staging file already exists" }
        try {
            temporary.outputStream().use { output ->
                output.write(bytes)
                output.flush()
                output.fd.sync()
            }
            Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE)
        } finally {
            temporary.delete()
        }
    }

    private fun root(context: Context) = File(context.noBackupFilesDir, "component-update-catalog")
}

private fun readRegular(file: File, maximum: Long): ByteArray {
    require(file.isFile && !Files.isSymbolicLink(file.toPath()) && file.length() <= maximum)
    return file.readBytes()
}

private fun requireRegularDirectory(directory: File) {
    require(directory.isDirectory && !Files.isSymbolicLink(directory.toPath()))
}

private val CACHED_CATALOG =
    Regex("^sgt-component-catalog-v\\d{6}-[0-9a-fA-F]{16}\\.json$")
private const val MAXIMUM_CACHED_CATALOGS = 64
