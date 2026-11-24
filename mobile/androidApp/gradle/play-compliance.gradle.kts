import groovy.json.JsonSlurper
import java.io.InputStream
import java.security.MessageDigest
import java.util.Properties
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import org.gradle.api.artifacts.VersionCatalogsExtension

data class CreationRuntimeBundleEntry(
    val bundlePath: String,
    val fileName: String,
    val sizeBytes: Long,
    val sha256: String,
)

data class CreationRuntimeBundleContract(
    val source: File,
    val featureEntries: List<CreationRuntimeBundleEntry>,
    val factorySignature: String,
)

fun Map<*, *>.requiredCreationMap(name: String): Map<*, *> =
    this[name] as? Map<*, *> ?: error("Creation runtime manifest is missing $name")

fun Map<*, *>.requiredCreationString(name: String): String =
    (this[name] as? String)?.takeIf { it.isNotBlank() }
        ?: error("Creation runtime manifest is missing $name")

fun loadCreationRuntimeBundleContract(repoRoot: File): CreationRuntimeBundleContract {
    val source = repoRoot.resolve("component-delivery/creation-runtime-v1.json")
    require(source.isFile) {
        "Play compliance requires the tracked creation runtime delivery contract"
    }
    @Suppress("UNCHECKED_CAST")
    val root = JsonSlurper().parse(source) as Map<*, *>
    val android = root.requiredCreationMap("android")
    val rawEntries = android["entries"] as? List<*>
        ?: error("Creation runtime manifest is missing Android entries")
    val nativeEntries = rawEntries.map { raw ->
        val entry = raw as? Map<*, *>
            ?: error("Creation runtime manifest contains an invalid Android entry")
        entry
    }.filter { entry ->
        entry.requiredCreationString("role") == "native_library"
    }.map { entry ->
        val installPath = entry.requiredCreationString("installPath")
            .replace('\\', '/')
        require(
            !installPath.startsWith('/') &&
                installPath.split('/').none { part ->
                    part.isEmpty() || part == "." || part == ".."
                },
        ) { "Creation runtime native install path is unsafe" }
        val fileName = installPath.substringAfterLast('/')
        val sizeBytes = (entry["sizeBytes"] as? Number)?.toLong()
            ?.takeIf { it > 0L }
            ?: error("Creation runtime manifest has an invalid native byte count")
        val sha256 = entry.requiredCreationString("sha256").lowercase()
        require(
            sha256.length == 64 &&
                sha256.all { it.isDigit() || it in 'a'..'f' },
        ) { "Creation runtime manifest has an invalid native checksum" }
        CreationRuntimeBundleEntry(
            bundlePath = "feature_creation_runtime/$installPath",
            fileName = fileName,
            sizeBytes = sizeBytes,
            sha256 = sha256,
        )
    }
    require(nativeEntries.isNotEmpty()) {
        "Creation runtime manifest has no native feature entries"
    }
    require(nativeEntries.map { it.bundlePath }.distinct().size == nativeEntries.size) {
        "Creation runtime manifest repeats a native feature path"
    }
    val factoryClass = android.requiredCreationString("factoryClass")
    val factorySignature = factoryClass.substringAfterLast('.')
    require(factorySignature.isNotBlank() && factorySignature != factoryClass) {
        "Creation runtime manifest has an invalid factory class"
    }
    return CreationRuntimeBundleContract(
        source = source,
        featureEntries = nativeEntries,
        factorySignature = factorySignature,
    )
}

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
val localTestingAdbPath = providers.gradleProperty("localTestingAdbPath")
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
            ) + localTestingAdbPath.orNull?.let { listOf("--adb=$it") }.orEmpty(),
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
    if (phoneControlDeviceSerial.isPresent) inputs.file(playDebugDeviceSpec)
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
            ) + localTestingAdbPath.orNull?.let { listOf("--adb=$it") }.orEmpty(),
        )
    }
}

tasks.register("verifyPlayReleaseCompliance") {
    group = "verification"
    description = "Verifies that the Play AAB keeps executable payloads out of its base module."
    dependsOn("bundlePlayRelease")
    notCompatibleWithConfigurationCache(
        "The Play bundle verifier reads its deliverable and tracked runtime contract at execution time.",
    )

    doLast {
        val bundle = project.layout.buildDirectory
            .file("outputs/bundle/playRelease/androidApp-play-release.aab")
            .get()
            .asFile
        require(bundle.isFile) { "Missing Play bundle: ${bundle.absolutePath}" }

        val creationRuntimeContract =
            loadCreationRuntimeBundleContract(rootProject.projectDir.parentFile)
        val forbiddenBaseNativeNames = listOf(
            "libc++_shared.so",
            "libpython.so",
            "libpython.zip.so",
            "libffmpeg.so",
            "libffmpeg.zip.so",
            "libffprobe.so",
        ) + creationRuntimeContract.featureEntries.map { it.fileName }
        val forbiddenDexStrings = listOf(
            "api.github.com/repos/nganlinh4/screen-goated-toolbox",
            "api.github.com/repos/yt-dlp",
            "youtubedl-android/releases/download",
            "github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles",
            "browser_download_url",
            "YoutubeDLUpdater",
            "updateYoutubeDL",
        ) + creationRuntimeContract.factorySignature
        val allowedFeatureModules = setOf("feature_creation_runtime")
        val forbiddenBaseResources = listOf(
            "base/res/raw/ytdlp",
            "base/assets/downloader-runtime/delivery.json",
            "base/assets/downloader-runtime/yt-dlp-release-key.asc",
        )
        val creationRuntimeDeliveryAsset =
            "feature_creation_runtime/assets/creation-runtime/delivery.json"

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
            require(featureNative.all { entry: ZipEntry -> entry.name.contains("/lib/arm64-v8a/") }) {
                "Play bundle contains an unsupported native ABI"
            }
            for (contractEntry in creationRuntimeContract.featureEntries) {
                val packaged = requireNotNull(zip.getEntry(contractEntry.bundlePath)) {
                    "Play creation runtime feature is missing ${contractEntry.bundlePath}"
                }
                require(packaged.size == contractEntry.sizeBytes) {
                    "Play creation runtime byte count mismatch for ${contractEntry.bundlePath}"
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
                require(actualSha256 == contractEntry.sha256) {
                    "Play creation runtime checksum mismatch for ${contractEntry.bundlePath}"
                }
            }
            val packagedCreationDelivery = requireNotNull(
                zip.getEntry(creationRuntimeDeliveryAsset),
            ) {
                "Play creation runtime feature is missing its delivery contract"
            }
            require(
                zip.getInputStream(packagedCreationDelivery).use { input -> input.readBytes() }
                    .contentEquals(creationRuntimeContract.source.readBytes()),
            ) { "Play packaged creation runtime delivery differs from its private owner" }

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
            require(creationDexText.contains(creationRuntimeContract.factorySignature)) {
                "Play creation runtime feature is missing its manifest-declared factory"
            }
        }
    }
}
