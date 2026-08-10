package dev.screengoated.toolbox.mobile.service.moonshine

import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.util.zip.ZipFile

internal class MoonshineBundleInstaller(
    private val client: OkHttpClient,
    private val cacheRoot: File,
) {
    suspend fun install(
        language: MoonshineLanguage,
        bundle: MoonshineModelBundle,
        modelDirectory: File,
        onProgress: (Float, String) -> Unit,
    ) {
        cacheRoot.mkdirs()
        val archivePart = File(cacheRoot, "${bundle.asset}.part")
        archivePart.delete()
        try {
            onProgress(0f, bundle.asset)
            download(bundle, archivePart, onProgress)
            installVerifiedArchive(bundle, language.modelFileContracts, archivePart, modelDirectory)
        } finally {
            archivePart.delete()
            language.modelFileContracts.forEach { contract ->
                File(modelDirectory, "${contract.name}.part").delete()
            }
        }
    }

    internal suspend fun installVerifiedArchive(
        bundle: MoonshineModelBundle,
        contracts: List<MoonshineModelFile>,
        archiveFile: File,
        modelDirectory: File,
    ) {
        check(modelDirectory.isDirectory || modelDirectory.mkdirs()) {
            "Could not create Moonshine model directory"
        }
        check(archiveFile.length() == bundle.byteCount) {
            "Downloaded ${bundle.asset} has the wrong size"
        }
        check(ManagedModelIntegrity.sha256(archiveFile) == bundle.sha256) {
            "Downloaded ${bundle.asset} failed integrity verification"
        }
        extract(contracts, archiveFile, modelDirectory)
    }

    private suspend fun download(
        bundle: MoonshineModelBundle,
        target: File,
        onProgress: (Float, String) -> Unit,
    ) {
        val request = Request.Builder().url(bundle.downloadUrl).build()
        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("HTTP ${response.code}")
            val body = response.body
            val contentLength = body.contentLength()
            check(contentLength < 0 || contentLength == bundle.byteCount) {
                "Download size for ${bundle.asset} does not match this build"
            }
            var downloaded = 0L
            target.outputStream().buffered().use { output ->
                val input = body.byteStream()
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    currentCoroutineContext().ensureActive()
                    val read = input.read(buffer)
                    if (read < 0) break
                    check(downloaded + read <= bundle.byteCount) {
                        "Download for ${bundle.asset} exceeds its limit"
                    }
                    output.write(buffer, 0, read)
                    downloaded += read
                    if (downloaded % (256 * 1024) < buffer.size) {
                        onProgress(downloaded.toFloat() / bundle.byteCount, bundle.asset)
                    }
                }
            }
        }
    }

    private suspend fun extract(
        contracts: List<MoonshineModelFile>,
        archiveFile: File,
        modelDirectory: File,
    ) {
        val expected = contracts.associateBy(MoonshineModelFile::name)
        val expectedEntries = expected.keys + NOTICE_ENTRIES.keys
        ZipFile(archiveFile).use { archive ->
            val entries = archive.entries().asSequence().toList()
            check(entries.none { it.isDirectory }) { "Moonshine bundle contains a directory" }
            check(entries.map { it.name }.toSet().size == entries.size) {
                "Moonshine bundle contains duplicate entries"
            }
            check(entries.map { it.name }.toSet() == expectedEntries) {
                "Moonshine bundle entries do not match this build"
            }
            NOTICE_ENTRIES.forEach { (name, byteCount) ->
                check(archive.getEntry(name).size == byteCount) {
                    "Moonshine bundle notice $name has the wrong size"
                }
            }
            contracts.forEach { contract ->
                val entry = archive.getEntry(contract.name)
                check(entry.size == contract.byteCount) {
                    "Moonshine bundle entry ${contract.name} has the wrong size"
                }
                val target = File(modelDirectory, contract.name)
                val part = File(modelDirectory, "${contract.name}.part")
                part.delete()
                archive.getInputStream(entry).use { input ->
                    part.outputStream().buffered().use { output ->
                        copyBounded(input, output, contract)
                    }
                }
                MoonshineModelIntegrity.finalizeVerifiedPart(part, target, contract)
            }
        }
    }

    private suspend fun copyBounded(
        input: java.io.InputStream,
        output: java.io.OutputStream,
        contract: MoonshineModelFile,
    ) {
        var written = 0L
        val buffer = ByteArray(64 * 1024)
        while (true) {
            currentCoroutineContext().ensureActive()
            val read = input.read(buffer)
            if (read < 0) break
            check(written + read <= contract.byteCount) {
                "Moonshine bundle entry ${contract.name} exceeds its limit"
            }
            output.write(buffer, 0, read)
            written += read
        }
        check(written == contract.byteCount) {
            "Moonshine bundle entry ${contract.name} is incomplete"
        }
    }

    private companion object {
        val NOTICE_ENTRIES = mapOf("LICENSE.txt" to 13_555L, "NOTICE.txt" to 804L)
    }
}
