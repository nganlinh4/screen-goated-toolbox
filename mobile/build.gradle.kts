import groovy.json.JsonSlurper
import java.io.InputStream
import java.security.MessageDigest
import java.util.zip.ZipFile

plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.android.dynamic.feature) apply false
    alias(libs.plugins.compose.compiler) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.multiplatform) apply false
    alias(libs.plugins.kotlin.serialization) apply false
}

val nativeRuntimeContractFile =
    rootProject.projectDir.parentFile.resolve("parity-fixtures/phone-control/native-runtime-contract.json")
val nativeRuntimeArchiveDir = rootProject.projectDir.resolve("androidApp/libs")

fun runtimeSha256(input: InputStream): String {
    val digest = MessageDigest.getInstance("SHA-256")
    input.use { source ->
        val buffer = ByteArray(1024 * 1024)
        while (true) {
            val read = source.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
    }
    return digest.digest().joinToString("") { byte -> "%02x".format(byte) }
}

tasks.register("verifyNativeRuntimeArchives") {
    group = "verification"
    description = "Verifies checked-in native runtime archives against the parity contract."
    inputs.file(nativeRuntimeContractFile)
    inputs.files(fileTree(nativeRuntimeArchiveDir) { include("*-runtime.zip") })

    doLast {
        @Suppress("UNCHECKED_CAST")
        val contract = JsonSlurper().parse(nativeRuntimeContractFile) as Map<String, Any?>
        require(contract.keys == setOf("schemaVersion", "abi", "archives")) {
            "Native runtime contract has unsupported top-level fields"
        }
        require((contract["schemaVersion"] as Number).toInt() == 1)
        require(contract["abi"] == "arm64-v8a")
        val archives = contract["archives"] as? List<*>
            ?: error("Native runtime contract archives must be an array")
        val expectedArchiveNames = linkedSetOf<String>()
        val expectedEngines = linkedSetOf<String>()
        archives.forEach { rawArchive ->
            @Suppress("UNCHECKED_CAST")
            val archive = rawArchive as? Map<String, Any?>
                ?: error("Native runtime archive must be an object")
            require(
                archive.keys == setOf(
                    "engine", "fileName", "byteCount", "sha256", "fullDelivery", "entries",
                ),
            ) { "Native runtime archive has unsupported fields" }
            val engine = archive["engine"] as String
            require(expectedEngines.add(engine)) { "Duplicate native runtime engine: $engine" }
            val fullDelivery = archive["fullDelivery"] as String
            require(fullDelivery in setOf("bundled_asset", "verified_download")) {
                "Unsupported Full native delivery for $engine: $fullDelivery"
            }
            require(
                (engine == "ort" && fullDelivery == "bundled_asset") ||
                    (engine != "ort" && fullDelivery == "verified_download"),
            ) { "Native runtime delivery differs for $engine" }
            val fileName = archive["fileName"] as String
            require(fileName == File(fileName).name && fileName.endsWith("-runtime.zip")) {
                "Native runtime archive name must be flat: $fileName"
            }
            require(expectedArchiveNames.add(fileName)) { "Duplicate native archive: $fileName" }
            val archiveFile = nativeRuntimeArchiveDir.resolve(fileName)
            require(archiveFile.isFile) { "Missing native runtime archive: ${archiveFile.absolutePath}" }
            val archiveByteCount = (archive["byteCount"] as Number).toLong()
            val archiveSha256 = archive["sha256"] as String
            require(archiveByteCount > 0L && archiveSha256.matches(Regex("[0-9a-f]{64}"))) {
                "$fileName has an invalid identity contract"
            }
            require(archiveFile.length() == archiveByteCount) {
                "$fileName byte count differs from contract"
            }
            require(runtimeSha256(archiveFile.inputStream()) == archiveSha256) {
                "$fileName SHA-256 differs from contract"
            }

            @Suppress("UNCHECKED_CAST")
            val expectedEntries = (archive["entries"] as List<Map<String, Any?>>)
                .associateBy { it["fileName"] as String }
            require(expectedEntries.size == (archive["entries"] as List<*>).size) {
                "$fileName contract contains duplicate members"
            }
            expectedEntries.forEach { (entryName, entry) ->
                require(entry.keys == setOf("fileName", "byteCount", "sha256")) {
                    "$fileName member has unsupported fields"
                }
                require(
                    entryName == File(entryName).name &&
                        !entryName.contains('/') && !entryName.contains('\\') &&
                        entryName.endsWith(".so"),
                ) { "$fileName contract member must be a flat library name: $entryName" }
                require(
                    (entry["byteCount"] as Number).toLong() > 0L &&
                        (entry["sha256"] as String).matches(Regex("[0-9a-f]{64}")),
                ) { "$fileName/$entryName has an invalid identity contract" }
            }
            ZipFile(archiveFile).use { zip ->
                val entries = zip.entries().asSequence().toList()
                require(entries.none { it.isDirectory }) { "$fileName contains directory entries" }
                val names = entries.map { it.name }
                require(names.size == names.toSet().size) { "$fileName contains duplicate entries" }
                require(names.toSet() == expectedEntries.keys) {
                    "$fileName members differ: expected=${expectedEntries.keys} actual=${names.toSet()}"
                }
                entries.forEach { zipEntry ->
                    val expected = requireNotNull(expectedEntries[zipEntry.name])
                    require(zipEntry.size == (expected["byteCount"] as Number).toLong()) {
                        "$fileName/${zipEntry.name} byte count differs from contract"
                    }
                    require(
                        runtimeSha256(zip.getInputStream(zipEntry)) ==
                            (expected["sha256"] as String),
                    ) {
                        "$fileName/${zipEntry.name} SHA-256 differs from contract"
                    }
                }
            }
        }
        require(expectedEngines == setOf("ort", "moonshine", "sherpa")) {
            "Native runtime engine set differs: $expectedEngines"
        }
        val checkedInArchives = nativeRuntimeArchiveDir.listFiles()
            .orEmpty()
            .filter { it.isFile && it.name.endsWith("-runtime.zip") }
            .map { it.name }
            .toSet()
        require(checkedInArchives == expectedArchiveNames) {
            "Checked-in native archives differ: expected=$expectedArchiveNames actual=$checkedInArchives"
        }
    }
}
