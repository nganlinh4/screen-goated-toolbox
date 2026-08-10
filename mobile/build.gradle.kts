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
val sherpaRuntimeSpecDir = rootProject.projectDir.resolve("native/sherpa-runtime")
val sherpaRuntimeBuildContractFile = sherpaRuntimeSpecDir.resolve("build-contract.json")
val sherpaRuntimeNoticesDir =
    sherpaRuntimeSpecDir.resolve("assets/third_party/sherpa-runtime")

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

fun elfSectionNames(bytes: ByteArray): Set<String> {
    fun u16(offset: Int): Int =
        (bytes[offset].toInt() and 0xff) or ((bytes[offset + 1].toInt() and 0xff) shl 8)
    fun u32(offset: Int): Long = (0 until 4).fold(0L) { value, index ->
        value or ((bytes[offset + index].toLong() and 0xffL) shl (index * 8))
    }
    fun u64(offset: Int): Long = (0 until 8).fold(0L) { value, index ->
        value or ((bytes[offset + index].toLong() and 0xffL) shl (index * 8))
    }
    require(bytes.size >= 64 && bytes.sliceArray(0..3).contentEquals(
        byteArrayOf(0x7f, 'E'.code.toByte(), 'L'.code.toByte(), 'F'.code.toByte()),
    )) { "Sherpa runtime is not ELF" }
    require(bytes[4].toInt() == 2 && bytes[5].toInt() == 1) {
        "Sherpa runtime must be little-endian ELF64"
    }
    require(u16(18) == 183) { "Sherpa runtime must target AArch64" }
    val sectionOffset = u64(40)
    val sectionEntrySize = u16(58)
    val sectionCount = u16(60)
    val stringTableIndex = u16(62)
    require(sectionEntrySize >= 64 && stringTableIndex < sectionCount)
    fun sectionHeader(index: Int): Int {
        val offset = sectionOffset + index.toLong() * sectionEntrySize
        require(offset >= 0 && offset + sectionEntrySize <= bytes.size)
        return offset.toInt()
    }
    val stringHeader = sectionHeader(stringTableIndex)
    val stringOffset = u64(stringHeader + 24).toInt()
    val stringSize = u64(stringHeader + 32).toInt()
    require(stringOffset >= 0 && stringSize >= 0 && stringOffset + stringSize <= bytes.size)
    return (0 until sectionCount).map { index ->
        val nameOffset = u32(sectionHeader(index)).toInt()
        require(nameOffset in 0 until stringSize)
        val start = stringOffset + nameOffset
        var end = start
        while (end < stringOffset + stringSize && bytes[end].toInt() != 0) end += 1
        bytes.copyOfRange(start, end).toString(Charsets.US_ASCII)
    }.toSet()
}

tasks.register("verifyNativeRuntimeArchives") {
    group = "verification"
    description = "Verifies checked-in native runtime archives against the parity contract."
    inputs.file(nativeRuntimeContractFile)
    inputs.files(fileTree(nativeRuntimeArchiveDir) { include("*-runtime.zip") })
    inputs.file(sherpaRuntimeBuildContractFile)
    inputs.dir(sherpaRuntimeNoticesDir)

    doLast {
        @Suppress("UNCHECKED_CAST")
        val contract = JsonSlurper().parse(nativeRuntimeContractFile) as Map<String, Any?>
        require(contract.keys == setOf("schemaVersion", "abi", "archives")) {
            "Native runtime contract has unsupported top-level fields"
        }
        require((contract["schemaVersion"] as Number).toInt() == 1)
        require(contract["abi"] == "arm64-v8a")
        @Suppress("UNCHECKED_CAST")
        val sherpaBuildContract =
            JsonSlurper().parse(sherpaRuntimeBuildContractFile) as Map<String, Any?>
        require((sherpaBuildContract["schemaVersion"] as Number).toInt() == 1)
        require(sherpaBuildContract["abi"] == contract["abi"])
        @Suppress("UNCHECKED_CAST")
        val sherpaArtifact = sherpaBuildContract["artifact"] as Map<String, Any?>
        @Suppress("UNCHECKED_CAST")
        val sherpaJavaUse = sherpaBuildContract["javaUse"] as Map<String, Any?>
        @Suppress("UNCHECKED_CAST")
        val sherpaElf = sherpaBuildContract["elf"] as Map<String, Any?>
        @Suppress("UNCHECKED_CAST")
        val sherpaOperatorGeneration =
            sherpaBuildContract["operatorGeneration"] as Map<String, Any?>
        @Suppress("UNCHECKED_CAST")
        val sherpaSourcePatch = sherpaBuildContract["sourcePatch"] as Map<String, Any?>
        val operatorConfig = sherpaRuntimeSpecDir.resolve(
            sherpaOperatorGeneration["configFile"] as String,
        )
        val operatorModels = sherpaRuntimeSpecDir.resolve(
            sherpaOperatorGeneration["modelsFile"] as String,
        )
        val sourcePatch = sherpaRuntimeSpecDir.resolve(sherpaSourcePatch["file"] as String)
        require(runtimeSha256(operatorConfig.inputStream()) ==
            sherpaOperatorGeneration["configSha256"] as String
        ) { "Sherpa reduced operator config differs from its build contract" }
        require(runtimeSha256(operatorModels.inputStream()) ==
            sherpaOperatorGeneration["modelsSha256"] as String
        ) { "Sherpa operator model inputs differ from their build contract" }
        require(runtimeSha256(sourcePatch.inputStream()) == sherpaSourcePatch["sha256"] as String) {
            "Sherpa source patch differs from its build contract"
        }
        @Suppress("UNCHECKED_CAST")
        val expectedNoticeNames = (sherpaBuildContract["noticeFiles"] as List<String>).toSet()
        val actualNoticeNames = sherpaRuntimeNoticesDir.listFiles().orEmpty()
            .filter(File::isFile)
            .onEach { require(it.length() > 0L) { "Empty Sherpa notice: ${it.name}" } }
            .map { it.name }
            .toSet()
        require(actualNoticeNames == expectedNoticeNames) {
            "Sherpa notice files differ: expected=$expectedNoticeNames actual=$actualNoticeNames"
        }
        val sherpaJavaSource = rootProject.projectDir.resolve(sherpaJavaUse["source"] as String)
        require(sherpaJavaSource.isFile) { "Missing Sherpa Java use owner: $sherpaJavaSource" }
        val sherpaJavaText = sherpaJavaSource.readText()
        val actualSherpaTypes = Regex("""com\.k2fsa\.sherpa\.onnx\.([A-Za-z0-9_]+)""")
            .findAll(sherpaJavaText)
            .map { it.groupValues[1] }
            .toSet()
        @Suppress("UNCHECKED_CAST")
        val expectedSherpaTypes = (sherpaJavaUse["types"] as List<String>).toSet()
        require(actualSherpaTypes == expectedSherpaTypes) {
            "Sherpa Java type use differs: expected=$expectedSherpaTypes actual=$actualSherpaTypes"
        }
        fun receiverMethods(receiver: String): Set<String> =
            Regex("""\b${Regex.escape(receiver)}\.([A-Za-z0-9_]+)\s*\(""")
                .findAll(sherpaJavaText)
                .map { it.groupValues[1] }
                .toSet()
        @Suppress("UNCHECKED_CAST")
        val expectedRecognizerMethods =
            (sherpaJavaUse["recognizerMethods"] as List<String>).toSet()
        @Suppress("UNCHECKED_CAST")
        val expectedStreamMethods = (sherpaJavaUse["streamMethods"] as List<String>).toSet()
        require(receiverMethods("recognizer") == expectedRecognizerMethods) {
            "Sherpa OnlineRecognizer method use differs from its native build contract"
        }
        require(receiverMethods("stream") == expectedStreamMethods) {
            "Sherpa OnlineStream method use differs from its native build contract"
        }
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
            require(fullDelivery == "verified_download") {
                "Full native runtime must use verified download delivery for $engine"
            }
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
            if (engine == "sherpa") {
                require(fileName == "sherpa-runtime.zip")
                require((sherpaArtifact["fileName"] as String) == "libsherpa-onnx-jni.so")
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
                if (engine == "sherpa") {
                    val entryName = sherpaArtifact["fileName"] as String
                    val nativeEntry = requireNotNull(zip.getEntry(entryName))
                    val nativeBytes = zip.getInputStream(nativeEntry).use { it.readBytes() }
                    require(nativeBytes.size.toLong() ==
                        (sherpaArtifact["byteCount"] as Number).toLong()
                    ) { "Sherpa ELF byte count differs from its build contract" }
                    require(runtimeSha256(nativeBytes.inputStream()) ==
                        sherpaArtifact["sha256"] as String
                    ) { "Sherpa ELF checksum differs from its build contract" }
                    require(nativeBytes.size.toLong() <=
                        (sherpaElf["maximumByteCount"] as Number).toLong()
                    ) { "Sherpa ELF exceeds its build-contract size ceiling" }
                    val elfText = nativeBytes.toString(Charsets.ISO_8859_1)
                    val sectionNames = elfSectionNames(nativeBytes)
                    val actualExports =
                        Regex("""Java_com_k2fsa_sherpa_onnx_[A-Za-z0-9_]+""")
                            .findAll(elfText)
                            .map { it.value }
                            .toSet()
                    @Suppress("UNCHECKED_CAST")
                    val expectedExports =
                        (sherpaBuildContract["jniExports"] as List<String>).toSet()
                    require(actualExports == expectedExports) {
                        "Sherpa JNI exports differ: expected=$expectedExports actual=$actualExports"
                    }
                    @Suppress("UNCHECKED_CAST")
                    val requiredNeeded = (sherpaElf["needed"] as List<String>).toSet()
                    @Suppress("UNCHECKED_CAST")
                    val forbiddenNeeded = (sherpaElf["forbiddenNeeded"] as List<String>).toSet()
                    require(requiredNeeded.all(elfText::contains)) {
                        "Sherpa ELF is missing a required Android dependency"
                    }
                    require(forbiddenNeeded.none(elfText::contains)) {
                        "Sherpa ELF gained a forbidden shared runtime dependency"
                    }
                    require(sectionNames.none { it.startsWith(".debug") } &&
                        ".symtab" !in sectionNames
                    ) {
                        "Sherpa ELF retains debug or static symbol sections"
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
