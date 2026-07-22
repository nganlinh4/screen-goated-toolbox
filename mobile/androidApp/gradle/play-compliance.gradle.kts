import groovy.json.JsonSlurper
import java.io.InputStream
import java.security.MessageDigest
import java.util.Properties
import javax.xml.parsers.DocumentBuilderFactory
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import org.gradle.api.artifacts.VersionCatalogsExtension

val kotlinRuntimeVersion = rootProject.extensions
    .getByType(VersionCatalogsExtension::class.java)
    .named("libs")
    .findVersion("kotlin")
    .get()
    .requiredVersion

val bundletoolRuntime = configurations.detachedConfiguration(
    dependencies.create("com.android.tools.build:bundletool:1.18.0"),
    dependencies.create("org.jetbrains.kotlin:kotlin-stdlib:$kotlinRuntimeVersion"),
)
val playDebugBundle = layout.buildDirectory.file(
    "outputs/bundle/playDebug/androidApp-play-debug.aab",
)
val phoneControlDeviceSerial = providers.gradleProperty("phoneControlDeviceSerial")
val phoneControlDeviceKey = phoneControlDeviceSerial
    .map { serial -> serial.replace(Regex("[^A-Za-z0-9._-]"), "_") }
    .orElse("all-devices")
val playDebugDeviceSpec = layout.buildDirectory.file(
    phoneControlDeviceKey.map { key ->
        "outputs/local-testing/playDebug/$key/device-spec.json"
    },
)
val playDebugLocalTestingApks = layout.buildDirectory.file(
    phoneControlDeviceKey.map { key ->
        "outputs/local-testing/playDebug/$key/androidApp-play-debug.apks"
    },
)

fun resolveAndroidAapt2(): File {
    val localProperties = Properties()
    val localPropertiesFile = rootProject.file("local.properties")
    if (localPropertiesFile.isFile) {
        localPropertiesFile.inputStream().use(localProperties::load)
    }
    val sdkRoots = listOfNotNull(
        System.getenv("ANDROID_HOME"),
        System.getenv("ANDROID_SDK_ROOT"),
        localProperties.getProperty("sdk.dir"),
        System.getProperty("user.home")?.let { "$it/android-sdk" },
        System.getenv("LOCALAPPDATA")?.let { "$it/Android/Sdk" },
    ).map(::File)
    val executable = if (System.getProperty("os.name").startsWith("Windows")) {
        "aapt2.exe"
    } else {
        "aapt2"
    }
    return sdkRoots.asSequence()
        .map { root -> root.resolve("build-tools/36.1.0/$executable") }
        .firstOrNull(File::isFile)
        ?: error("Android build-tools 36.1.0 aapt2 was not found")
}

val capturePlayDebugDeviceSpec = tasks.register<JavaExec>(
    "capturePlayDebugDeviceSpec",
) {
    group = "verification"
    description = "Captures the exact BundleTool device spec for a Phone Control target."
    classpath(bundletoolRuntime)
    mainClass.set("com.android.tools.build.bundletool.BundleToolMain")
    inputs.property("phoneControlDeviceSerial", phoneControlDeviceSerial)
    outputs.file(playDebugDeviceSpec)
    outputs.upToDateWhen { false }
    onlyIf { phoneControlDeviceSerial.isPresent }
    doFirst {
        val serial = phoneControlDeviceSerial.orNull
            ?.takeIf(String::isNotBlank)
            ?: error("-PphoneControlDeviceSerial=<exact adb serial> is required")
        playDebugDeviceSpec.get().asFile.parentFile.mkdirs()
        setArgs(
            listOf(
                "get-device-spec",
                "--device-id=$serial",
                "--output=${playDebugDeviceSpec.get().asFile.absolutePath}",
                "--overwrite",
            ),
        )
    }
}

val buildPlayDebugLocalTestingApks = tasks.register<JavaExec>(
    "buildPlayDebugLocalTestingApks",
) {
    group = "verification"
    description = "Builds locally deliverable Play splits for emulator/device testing."
    dependsOn("bundlePlayDebug")
    dependsOn(capturePlayDebugDeviceSpec)
    classpath(bundletoolRuntime)
    mainClass.set("com.android.tools.build.bundletool.BundleToolMain")
    inputs.file(playDebugBundle)
    inputs.file(playDebugDeviceSpec).optional()
    inputs.property(
        "phoneControlDeviceSerial",
        phoneControlDeviceSerial.orElse("all-devices"),
    )
    outputs.file(playDebugLocalTestingApks)
    doFirst {
        playDebugLocalTestingApks.get().asFile.parentFile.mkdirs()
        val arguments = mutableListOf(
            "build-apks",
            "--bundle=${playDebugBundle.get().asFile.absolutePath}",
            "--output=${playDebugLocalTestingApks.get().asFile.absolutePath}",
            "--aapt2=${resolveAndroidAapt2().absolutePath}",
            "--local-testing",
            "--overwrite",
        )
        phoneControlDeviceSerial.orNull?.let {
            arguments += "--device-spec=${playDebugDeviceSpec.get().asFile.absolutePath}"
        }
        setArgs(arguments)
    }
}

tasks.register<JavaExec>("installPlayDebugLocalTesting") {
    group = "verification"
    description = "Installs the Play debug base with on-demand splits available locally."
    dependsOn(buildPlayDebugLocalTestingApks)
    classpath(bundletoolRuntime)
    mainClass.set("com.android.tools.build.bundletool.BundleToolMain")
    inputs.file(playDebugLocalTestingApks)
    doFirst {
        val serial = phoneControlDeviceSerial.orNull
            ?.takeIf(String::isNotBlank)
            ?: error("-PphoneControlDeviceSerial=<exact adb serial> is required")
        setArgs(
            listOf(
                "install-apks",
                "--apks=${playDebugLocalTestingApks.get().asFile.absolutePath}",
                "--device-id=$serial",
            ),
        )
    }
}

tasks.register("verifyPlayReleaseCompliance") {
    group = "verification"
    description = "Verifies that the Play AAB keeps executable payloads out of its base module."
    dependsOn("bundlePlayRelease")
    dependsOn(rootProject.tasks.named("verifyNativeRuntimeArchives"))

    doLast {
        val bundle = project.layout.buildDirectory
            .file("outputs/bundle/playRelease/androidApp-play-release.aab")
            .get()
            .asFile
        require(bundle.isFile) { "Missing Play bundle: ${bundle.absolutePath}" }

        val forbiddenBaseNativeNames = listOf(
            "libc++_shared.so",
            "libonnxruntime.so",
            "libonnxruntime_real.so",
            "libmoonshine.so",
            "libmoonshine-jni.so",
            "libsherpa-onnx-jni.so",
            "libpython.so",
            "libpython.zip.so",
            "libffmpeg.so",
            "libffmpeg.zip.so",
            "libffprobe.so",
            "libsgt_creation_glb.so",
        )
        val forbiddenDexStrings = listOf(
            "api.github.com/repos/nganlinh4/screen-goated-toolbox",
            "api.github.com/repos/yt-dlp",
            "youtubedl-android/releases/download",
            "raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/mobile/androidApp/libs",
            "browser_download_url",
            "YoutubeDLUpdater",
            "updateYoutubeDL",
            "studio.tripo3d.ai",
            "api.tripo3d.ai",
            "temp-mail.org/en/",
            "www.svgai.org/",
            "Depth preview model has no depth output",
        )
        val allowedFeatureModules = setOf(
            "feature_asr_ort",
            "feature_asr_moonshine",
            "feature_asr_sherpa",
            "feature_creation_runtime",
            "feature_native_cpp",
        )
        val requiredNativeOwners = mapOf(
            "libonnxruntime.so" to "feature_asr_ort/lib/arm64-v8a/libonnxruntime.so",
            "libonnxruntime_real.so" to
                "feature_asr_ort/lib/arm64-v8a/libonnxruntime_real.so",
            "libmoonshine-jni.so" to
                "feature_asr_moonshine/lib/arm64-v8a/libmoonshine-jni.so",
            "libmoonshine.so" to "feature_asr_moonshine/lib/arm64-v8a/libmoonshine.so",
            "libsherpa-onnx-jni.so" to
                "feature_asr_sherpa/lib/arm64-v8a/libsherpa-onnx-jni.so",
            "libc++_shared.so" to "feature_native_cpp/lib/arm64-v8a/libc++_shared.so",
        )
        val nativeRuntimeContractFile = rootProject.projectDir.parentFile
            .resolve("parity-fixtures/phone-control/native-runtime-contract.json")
        @Suppress("UNCHECKED_CAST")
        val nativeRuntimeContract =
            JsonSlurper().parse(nativeRuntimeContractFile) as Map<String, Any?>
        @Suppress("UNCHECKED_CAST")
        val nativeRuntimeEntries =
            (nativeRuntimeContract["archives"] as List<Map<String, Any?>>)
                .flatMap { archive -> archive["entries"] as List<Map<String, Any?>> }
                .associateBy { entry -> entry["fileName"] as String }
        require(nativeRuntimeEntries.keys == requiredNativeOwners.keys) {
            "Play native owners differ from the shared runtime contract"
        }
        val nativeRuntimeContractAsset = "base/assets/native-runtime/contract.json"
        val forbiddenBaseResources = listOf("base/res/raw/ytdlp")
        val detectorAssetPath = "feature_asr_ort/assets/ui_detector/ui-detr-1.onnx"
        val detectorAssetBytes = 131_216_489L
        val detectorAssetSha256 =
            "1892092320cd55fd182c6afd76ae5bb0fb9695f5fcdf0ba875c1f68d49792ff4"
        val creationNativePath =
            "feature_creation_runtime/lib/arm64-v8a/libsgt_creation_glb.so"
        val creationNativeBytes = 342_304L
        val creationNativeSha256 =
            "6520b51e703b953ffed3509310f693c68ceb107b37908dfac4c44eb9f42c55cc"
        val creationRuntimeSignatures = listOf(
            "AndroidCreationRuntimeFactory",
            "studio.tripo3d.ai",
            "api.tripo3d.ai",
            "temp-mail.org/en/",
            "www.svgai.org/",
            "Depth preview model has no depth output",
            "MeshoptNative",
        )

        val playManifest = project.file("src/play/AndroidManifest.xml")
        val manifestDocument = DocumentBuilderFactory.newInstance().apply {
            isNamespaceAware = true
        }.newDocumentBuilder().parse(playManifest)
        val activityNodes = manifestDocument.getElementsByTagName("activity")
        val confirmationActivities = (0 until activityNodes.length).map { activityNodes.item(it) }
            .filter { node ->
                node.attributes.getNamedItemNS(
                    "http://schemas.android.com/apk/res/android",
                    "name",
                )?.nodeValue ==
                    ".service.nativelibs.PlaySplitInstallConfirmationActivity"
            }
        require(confirmationActivities.size == 1) {
            "Play split confirmation proxy must be declared exactly once"
        }
        require(
            confirmationActivities.single().attributes.getNamedItemNS(
                "http://schemas.android.com/apk/res/android",
                "exported",
            )?.nodeValue == "false",
        ) { "Play split confirmation proxy must be non-exported" }

        ZipFile(bundle).use { zip ->
            val entries = zip.entries().asSequence().toList()
            val baseNative = entries.filter { entry: ZipEntry -> entry.name.startsWith("base/lib/") }
            require(baseNative.none { entry: ZipEntry ->
                forbiddenBaseNativeNames.any { name -> entry.name.endsWith(name) }
            }) { "Play base contains an on-demand native payload" }

            val retainedResources = forbiddenBaseResources.filter { name -> zip.getEntry(name) != null }
            require(retainedResources.isEmpty()) {
                "Play base retains forbidden resources: $retainedResources"
            }

            val featureModules = entries
                .map { entry: ZipEntry -> entry.name.substringBefore('/') }
                .filter { name -> name.startsWith("feature") }
                .toSet()
            require(featureModules == allowedFeatureModules) {
                "Play bundle feature modules differ: expected=$allowedFeatureModules actual=$featureModules"
            }

            val featureNative = entries.filter { entry: ZipEntry ->
                entry.name.contains("/lib/") && !entry.name.startsWith("base/")
            }
            require(featureNative.isNotEmpty()) { "Play bundle has no native feature payloads" }
            require(featureNative.all { entry: ZipEntry -> entry.name.contains("/lib/arm64-v8a/") }) {
                "Play bundle contains an unsupported native ABI"
            }
            requiredNativeOwners.forEach { (fileName, expectedPath) ->
                val matching = entries.filter { entry -> entry.name.endsWith("/$fileName") }
                require(matching.map { it.name } == listOf(expectedPath)) {
                    "Play native ownership mismatch for $fileName: ${matching.map { it.name }}"
                }
                val packaged = matching.single()
                val contractEntry = requireNotNull(nativeRuntimeEntries[fileName])
                require(packaged.size == (contractEntry["byteCount"] as Number).toLong()) {
                    "Play native byte count mismatch for $fileName: ${packaged.size}"
                }
                val digest = MessageDigest.getInstance("SHA-256")
                zip.getInputStream(packaged).use { input ->
                    val buffer = ByteArray(1024 * 1024)
                    while (true) {
                        val read = input.read(buffer)
                        if (read < 0) break
                        digest.update(buffer, 0, read)
                    }
                }
                val actualSha256 = digest.digest().joinToString("") { byte ->
                    "%02x".format(byte)
                }
                require(actualSha256 == (contractEntry["sha256"] as String)) {
                    "Play native checksum mismatch for $fileName: $actualSha256"
                }
            }
            val creationNativeEntries = entries.filter { entry ->
                entry.name.endsWith("/libsgt_creation_glb.so")
            }
            require(creationNativeEntries.map { it.name } == listOf(creationNativePath)) {
                "Play creation native ownership mismatch: ${creationNativeEntries.map { it.name }}"
            }
            val creationNative = creationNativeEntries.single()
            require(creationNative.size == creationNativeBytes) {
                "Play creation native byte count mismatch: ${creationNative.size}"
            }
            val creationDigest = MessageDigest.getInstance("SHA-256")
            zip.getInputStream(creationNative).use { input ->
                val buffer = ByteArray(1024 * 1024)
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    creationDigest.update(buffer, 0, read)
                }
            }
            val actualCreationSha256 = creationDigest.digest().joinToString("") { byte ->
                "%02x".format(byte)
            }
            require(actualCreationSha256 == creationNativeSha256) {
                "Play creation native checksum mismatch: $actualCreationSha256"
            }
            require(zip.getEntry("base/lib/arm64-v8a/libonnxruntime4j_jni.so") != null) {
                "Play base is missing the Java ORT JNI bridge"
            }
            val packagedRuntimeContract = requireNotNull(zip.getEntry(nativeRuntimeContractAsset)) {
                "Play base is missing the shared native runtime contract"
            }
            require(
                zip.getInputStream(packagedRuntimeContract).use { input -> input.readBytes() }
                    .contentEquals(nativeRuntimeContractFile.readBytes()),
            ) { "Play packaged native runtime contract differs from its shared owner" }

            val detectorEntries = entries.filter { entry ->
                entry.name.endsWith("/assets/ui_detector/ui-detr-1.onnx")
            }
            require(detectorEntries.map { it.name } == listOf(detectorAssetPath)) {
                "Play detector ownership mismatch: ${detectorEntries.map { it.name }}"
            }
            val detectorEntry = detectorEntries.single()
            require(detectorEntry.size == detectorAssetBytes) {
                "Play detector size mismatch: ${detectorEntry.size}"
            }
            val detectorDigest = MessageDigest.getInstance("SHA-256")
            zip.getInputStream(detectorEntry).use { input ->
                val buffer = ByteArray(1024 * 1024)
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    detectorDigest.update(buffer, 0, read)
                }
            }
            val actualDetectorSha256 = detectorDigest.digest().joinToString("") { byte ->
                "%02x".format(byte)
            }
            require(actualDetectorSha256 == detectorAssetSha256) {
                "Play detector checksum mismatch: $actualDetectorSha256"
            }

            val baseDexEntries = entries.filter { entry: ZipEntry ->
                entry.name.matches(Regex("base/dex/classes\\d*\\.dex"))
            }
            require(baseDexEntries.isNotEmpty()) { "Play bundle is missing base dex" }
            for (dexEntry in baseDexEntries) {
                val dexText = zip.getInputStream(dexEntry).use { input: InputStream ->
                    input.readBytes().toString(Charsets.ISO_8859_1)
                }
                val retained = forbiddenDexStrings.filter { signature -> dexText.contains(signature) }
                require(retained.isEmpty()) {
                    "Play base dex ${dexEntry.name} retains forbidden signatures: $retained"
                }
            }

            val creationDexEntries = entries.filter { entry: ZipEntry ->
                entry.name.matches(Regex("feature_creation_runtime/dex/classes\\d*\\.dex"))
            }
            require(creationDexEntries.isNotEmpty()) {
                "Play creation runtime feature is missing executable code"
            }
            val creationDexText = creationDexEntries.joinToString(separator = "") { dexEntry ->
                zip.getInputStream(dexEntry).use { input: InputStream ->
                    input.readBytes().toString(Charsets.ISO_8859_1)
                }
            }
            val missingCreationSignatures = creationRuntimeSignatures.filterNot(creationDexText::contains)
            require(missingCreationSignatures.isEmpty()) {
                "Play creation runtime feature is incomplete: $missingCreationSignatures"
            }
        }
    }
}
