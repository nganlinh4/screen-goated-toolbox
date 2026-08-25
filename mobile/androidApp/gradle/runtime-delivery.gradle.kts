import groovy.json.JsonSlurper
import org.gradle.api.tasks.Sync

enum class CreationRuntimeDeliveryChannel {
    Production,
    Staging,
}

data class CreationRuntimeDeliverySelection(
    val manifest: File,
    val channel: CreationRuntimeDeliveryChannel,
)

val creationRuntimeProductionPrefix =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/" +
        "download/sgt-runtime-bundles/"
val creationRuntimeStagingPrefix =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/" +
        "download/sgt-runtime-staging/"
val creationRuntimeStagingRelativePath =
    "SGT-Development/cache/staging/contracts/component-delivery/creation-runtime-v1.json"

fun requestedCreationRuntimeDeliveryChannel(value: String?): CreationRuntimeDeliveryChannel =
    when (value.orEmpty()) {
        "", "production" -> CreationRuntimeDeliveryChannel.Production
        "staging" -> CreationRuntimeDeliveryChannel.Staging
        else -> error("Unsupported SGT_COMPONENT_DELIVERY_CHANNEL ${value?.let { "\"$it\"" }}")
    }

fun stagingCreationRuntimeManifest(localAppData: String?): File? {
    val configured = localAppData?.takeIf(String::isNotBlank) ?: return null
    val root = File(configured)
    require(root.isAbsolute) { "LOCALAPPDATA must be absolute for staging delivery" }
    return root.toPath().normalize().resolve(creationRuntimeStagingRelativePath).toFile()
}

fun selectCreationRuntimeDeliveryManifest(
    production: File,
    requested: CreationRuntimeDeliveryChannel,
    localAppData: String?,
): CreationRuntimeDeliverySelection {
    require(production.isFile) {
        "Tracked creation runtime delivery contract is required: $production"
    }
    if (requested == CreationRuntimeDeliveryChannel.Production) {
        return CreationRuntimeDeliverySelection(production, requested)
    }
    val staged = stagingCreationRuntimeManifest(localAppData)
    if (staged?.isFile != true) {
        return CreationRuntimeDeliverySelection(production, CreationRuntimeDeliveryChannel.Production)
    }
    val stagedChannel = creationRuntimeAndroidChannel(staged)
    if (stagedChannel == CreationRuntimeDeliveryChannel.Staging) {
        return CreationRuntimeDeliverySelection(staged, stagedChannel)
    }
    val productionAndroid = creationRuntimeAndroidMap(production)
    val stagedAndroid = creationRuntimeAndroidMap(staged)
    require(stagedAndroid == productionAndroid) {
        "Production-tagged Android staging records must match the tracked production contract"
    }
    return CreationRuntimeDeliverySelection(production, CreationRuntimeDeliveryChannel.Production)
}

fun Map<*, *>.requiredCreationRuntimeMap(name: String): Map<*, *> =
    this[name] as? Map<*, *> ?: error("Creation runtime manifest is missing $name")

fun Map<*, *>.requiredCreationRuntimeString(name: String): String =
    (this[name] as? String)?.takeIf(String::isNotBlank)
        ?: error("Creation runtime manifest is missing $name")

fun creationRuntimeAndroidMap(source: File): Map<*, *> {
    val root = JsonSlurper().parse(source) as? Map<*, *>
        ?: error("Creation runtime manifest root must be an object")
    return root.requiredCreationRuntimeMap("android")
}

fun creationRuntimeAndroidRecordChannel(
    record: Map<*, *>,
    role: String,
): CreationRuntimeDeliveryChannel {
    val asset = (record["asset"] ?: record["file"]) as? String
        ?: error("Creation runtime manifest is missing $role asset")
    val sha256 = record.requiredCreationRuntimeString("sha256")
    val url = record.requiredCreationRuntimeString("downloadUrl")
    require(asset.isNotEmpty() && '/' !in asset && '\\' !in asset && asset !in setOf(".", "..")) {
        "Creation runtime $role asset must be a file name"
    }
    require(sha256.matches(Regex("[0-9a-f]{64}"))) {
        "Creation runtime manifest has invalid $role SHA-256"
    }
    val expectedAsset = when (role) {
        "android.full" -> "sgt-creation-runtime-android-arm64-${sha256.take(16)}.zip"
        "android.play" -> "sgt-creation-runtime-android-${sha256.take(16)}.aar"
        else -> error("Unsupported creation runtime role $role")
    }
    require(asset == expectedAsset) { "Creation runtime $role asset is not content-addressed" }
    return when (url) {
        "$creationRuntimeProductionPrefix$asset" -> CreationRuntimeDeliveryChannel.Production
        "$creationRuntimeStagingPrefix$asset" -> CreationRuntimeDeliveryChannel.Staging
        else -> error("Creation runtime $role URL does not use a supported delivery tag")
    }
}

fun creationRuntimeAndroidChannel(source: File): CreationRuntimeDeliveryChannel {
    val android = creationRuntimeAndroidMap(source)
    val full = creationRuntimeAndroidRecordChannel(
        android.requiredCreationRuntimeMap("full"),
        "android.full",
    )
    val play = creationRuntimeAndroidRecordChannel(
        android.requiredCreationRuntimeMap("play"),
        "android.play",
    )
    require(full == play) {
        "Creation runtime Android Full and Play records must use the same delivery tag"
    }
    return full
}

fun validateCreationRuntimeDeliveryManifest(
    source: File,
    channel: CreationRuntimeDeliveryChannel,
    hostVersion: String,
) {
    val root = JsonSlurper().parse(source) as? Map<*, *>
        ?: error("Creation runtime manifest root must be an object")
    require((root["schemaVersion"] as? Number)?.toInt() == 1) {
        "Unsupported creation runtime delivery schema"
    }
    require(root.requiredCreationRuntimeString("hostVersion") == hostVersion) {
        "Creation runtime manifest targets another app version"
    }
    require(creationRuntimeAndroidChannel(source) == channel) {
        "Creation runtime Android URL tag differs from the selected ${channel.name.lowercase()} channel"
    }
}

fun isDebugOnlyCreationRuntimeTaskGraph(taskPaths: Collection<String>): Boolean {
    val taskNames = taskPaths.map { it.substringAfterLast(':') }
    return taskNames.any { it.contains("Debug") } && taskNames.none { it.contains("Release") }
}

val generatedNativeRuntimeContractAssets =
    layout.buildDirectory.dir("generated/nativeRuntimeContractAssets")
val generatedComponentUpdateTrustAssets =
    layout.buildDirectory.dir("generated/componentUpdateTrustAssets")
val generatedFullCreationRuntimeDeliveryAssets =
    layout.buildDirectory.dir("generated/fullCreationRuntimeDeliveryAssets")
val generatedFullDownloaderRuntimeDeliveryAssets =
    layout.buildDirectory.dir("generated/fullDownloaderRuntimeDeliveryAssets")
val generatedFullDownloaderLauncherJniLibs =
    layout.buildDirectory.dir("generated/fullDownloaderLauncherJniLibs")
val sharedCreationModelViewerAssets = rootProject.projectDir.parentFile
    .resolve("3d-generator-ui/viewer-dist")
val nativeRuntimeContractSource = rootProject.projectDir.parentFile
    .resolve("parity-fixtures/phone-control/native-runtime-contract.json")
val componentUpdatePublicKey = rootProject.projectDir.parentFile
    .resolve("component-delivery/update-catalog-p256-public-key.hex")
val trackedCreationRuntimeDeliveryManifest = rootProject.projectDir.parentFile.resolve(
    "component-delivery/creation-runtime-v1.json",
)
val creationRuntimeHostVersion = Regex("(?m)^version\\s*=\\s*\"([^\"]+)\"")
    .find(rootProject.projectDir.parentFile.resolve("Cargo.toml").readText())
    ?.groupValues
    ?.get(1)
    ?: error("Root Cargo package version is missing")
val requestedCreationRuntimeDeliveryChannel = requestedCreationRuntimeDeliveryChannel(
    providers.environmentVariable("SGT_COMPONENT_DELIVERY_CHANNEL").orNull,
)
val creationRuntimeDeliverySelection = selectCreationRuntimeDeliveryManifest(
    trackedCreationRuntimeDeliveryManifest,
    requestedCreationRuntimeDeliveryChannel,
    providers.environmentVariable("LOCALAPPDATA").orNull,
)
val creationRuntimeDeliveryManifest = creationRuntimeDeliverySelection.manifest
validateCreationRuntimeDeliveryManifest(
    creationRuntimeDeliveryManifest,
    creationRuntimeDeliverySelection.channel,
    creationRuntimeHostVersion,
)
rootProject.extensions.extraProperties.set(
    "sgtCreationRuntimeDeliveryManifest",
    creationRuntimeDeliveryManifest.absolutePath,
)
rootProject.extensions.extraProperties.set(
    "sgtCreationRuntimeDeliveryChannel",
    creationRuntimeDeliverySelection.channel.name,
)
if (requestedCreationRuntimeDeliveryChannel == CreationRuntimeDeliveryChannel.Staging) {
    gradle.taskGraph.whenReady {
        require(isDebugOnlyCreationRuntimeTaskGraph(allTasks.map { it.path })) {
            "Staging component delivery is allowed only for debug Android builds"
        }
    }
}
val downloaderRuntimeDeliveryManifest = projectDir.resolve("delivery/downloader-runtime.json")
val downloaderLauncherSourceRoot = rootProject.projectDir.resolve("../../youtubedl-android")
val downloaderLauncherContract = linkedMapOf(
    "library/src/main/jniLibs/arm64-v8a/libpython.so" to Pair(
        5_744L,
        "8184bd26986955434996a971f73af9e878ee50a7b2a14609c9af7cfc70e7ad58",
    ),
    "ffmpeg/src/main/jniLibs/arm64-v8a/libffmpeg.so" to Pair(
        5_336L,
        "3441cea3739fe72553fbd51bb15a8f949ac8bb15cf9d1aab53d9637b3b4b30cb",
    ),
    "ffmpeg/src/main/jniLibs/arm64-v8a/libffprobe.so" to Pair(
        5_376L,
        "dba2b6cd18cd32bd12b55db17cb20db29a8ee72a33c998921846f4ec2d03a70a",
    ),
)

val androidAssetVerifier = rootProject.projectDir.resolve("scripts/verify_android_build_assets.py")

val stageNativeRuntimeContract by tasks.registering(Sync::class) {
    dependsOn(rootProject.tasks.named("verifyNativeRuntimeArchives"))
    from(nativeRuntimeContractSource) { rename { "contract.json" } }
    into(generatedNativeRuntimeContractAssets.map { it.dir("native-runtime") })
}

val verifyComponentUpdateTrust by tasks.registering(Exec::class) {
    inputs.file(componentUpdatePublicKey)
    inputs.file(androidAssetVerifier)
    commandLine(
        "py", "-3", androidAssetVerifier.absolutePath,
        "component-key", "--file", componentUpdatePublicKey.absolutePath,
    )
}

val stageComponentUpdateTrust by tasks.registering(Sync::class) {
    dependsOn(verifyComponentUpdateTrust)
    from(componentUpdatePublicKey) { rename { "public-key.hex" } }
    into(generatedComponentUpdateTrustAssets.map { it.dir("component-update") })
}

val verifyCreationModelViewerAssets by tasks.registering(Exec::class) {
    inputs.dir(sharedCreationModelViewerAssets)
    inputs.file(androidAssetVerifier)
    commandLine(
        "py", "-3", androidAssetVerifier.absolutePath,
        "viewer", "--root", sharedCreationModelViewerAssets.absolutePath,
    )
}

val stageFullCreationRuntimeDelivery by tasks.registering(Sync::class) {
    inputs.file(creationRuntimeDeliveryManifest)
    inputs.property("creationRuntimeDeliveryChannel", creationRuntimeDeliverySelection.channel.name)
    from(creationRuntimeDeliveryManifest) { rename { "delivery.json" } }
    into(generatedFullCreationRuntimeDeliveryAssets.map { it.dir("creation-runtime") })
}

val debugCreationRuntimeStagingTestLocalAppData =
    layout.buildDirectory.dir("creationRuntimeDeliverySelectionTest/localAppData")

val prepareDebugCreationRuntimeStagingSelectionFixture by tasks.registering {
    group = "verification"
    description = "Prepares a remote-only staging contract fixture for a focused Gradle rerun."
    inputs.file(trackedCreationRuntimeDeliveryManifest)
    outputs.file(debugCreationRuntimeStagingTestLocalAppData.map {
        it.file(creationRuntimeStagingRelativePath)
    })
    doLast {
        val output = debugCreationRuntimeStagingTestLocalAppData.get().asFile
            .resolve(creationRuntimeStagingRelativePath)
        output.parentFile.mkdirs()
        output.writeText(
            trackedCreationRuntimeDeliveryManifest.readText().replace(
                creationRuntimeProductionPrefix,
                creationRuntimeStagingPrefix,
            ),
        )
    }
}

val testDebugCreationRuntimeDeliverySelection by tasks.registering {
    group = "verification"
    description = "Tests production, staging, fallback, channel, and build-type selection."
    inputs.file(trackedCreationRuntimeDeliveryManifest)
    doLast {
        require(
            requestedCreationRuntimeDeliveryChannel(null) ==
                CreationRuntimeDeliveryChannel.Production,
        )
        require(
            requestedCreationRuntimeDeliveryChannel("production") ==
                CreationRuntimeDeliveryChannel.Production,
        )
        require(
            requestedCreationRuntimeDeliveryChannel("staging") ==
                CreationRuntimeDeliveryChannel.Staging,
        )
        require(runCatching { requestedCreationRuntimeDeliveryChannel("preview") }.isFailure)

        val testLocalAppData = temporaryDir.resolve("case-${System.nanoTime()}")
        val fallback = selectCreationRuntimeDeliveryManifest(
            trackedCreationRuntimeDeliveryManifest,
            CreationRuntimeDeliveryChannel.Staging,
            testLocalAppData.absolutePath,
        )
        require(fallback.manifest == trackedCreationRuntimeDeliveryManifest)
        require(fallback.channel == CreationRuntimeDeliveryChannel.Production)
        validateCreationRuntimeDeliveryManifest(
            fallback.manifest,
            fallback.channel,
            creationRuntimeHostVersion,
        )

        val staged = requireNotNull(stagingCreationRuntimeManifest(testLocalAppData.absolutePath))
        staged.parentFile.mkdirs()
        staged.writeText(trackedCreationRuntimeDeliveryManifest.readText())
        val unchangedFallback = selectCreationRuntimeDeliveryManifest(
            trackedCreationRuntimeDeliveryManifest,
            CreationRuntimeDeliveryChannel.Staging,
            testLocalAppData.absolutePath,
        )
        require(unchangedFallback.manifest == trackedCreationRuntimeDeliveryManifest)
        require(unchangedFallback.channel == CreationRuntimeDeliveryChannel.Production)

        staged.writeText(
            trackedCreationRuntimeDeliveryManifest.readText().replace(
                creationRuntimeProductionPrefix,
                creationRuntimeStagingPrefix,
            ),
        )
        val selected = selectCreationRuntimeDeliveryManifest(
            trackedCreationRuntimeDeliveryManifest,
            CreationRuntimeDeliveryChannel.Staging,
            testLocalAppData.absolutePath,
        )
        require(selected.manifest.toPath().normalize() == staged.toPath().normalize())
        require(selected.channel == CreationRuntimeDeliveryChannel.Staging)
        validateCreationRuntimeDeliveryManifest(
            selected.manifest,
            selected.channel,
            creationRuntimeHostVersion,
        )

        val wrongTag = temporaryDir.resolve("wrong-tag-${System.nanoTime()}.json")
        wrongTag.writeText(staged.readText().replace(
            creationRuntimeStagingPrefix,
            creationRuntimeProductionPrefix,
        ))
        require(runCatching {
            validateCreationRuntimeDeliveryManifest(
                wrongTag,
                CreationRuntimeDeliveryChannel.Staging,
                creationRuntimeHostVersion,
            )
        }.isFailure)
        val mixedTag = temporaryDir.resolve("mixed-tag-${System.nanoTime()}.json")
        val stagedFullAsset = creationRuntimeAndroidMap(staged)
            .requiredCreationRuntimeMap("full")
            .requiredCreationRuntimeString("asset")
        mixedTag.writeText(staged.readText().replace(
            "$creationRuntimeStagingPrefix$stagedFullAsset",
            "$creationRuntimeProductionPrefix$stagedFullAsset",
        ))
        require(runCatching { creationRuntimeAndroidChannel(mixedTag) }.isFailure)

        val mutatedProduction = temporaryDir.resolve("mutated-production-${System.nanoTime()}")
        val mutatedStaged = requireNotNull(stagingCreationRuntimeManifest(
            mutatedProduction.absolutePath,
        ))
        mutatedStaged.parentFile.mkdirs()
        mutatedStaged.writeText(
            trackedCreationRuntimeDeliveryManifest.readText().replace(
                "\"factoryClass\"",
                "\"changedFactoryClass\"",
            ),
        )
        require(runCatching {
            selectCreationRuntimeDeliveryManifest(
                trackedCreationRuntimeDeliveryManifest,
                CreationRuntimeDeliveryChannel.Staging,
                mutatedProduction.absolutePath,
            )
        }.isFailure)
        require(runCatching { stagingCreationRuntimeManifest("relative") }.isFailure)
        require(isDebugOnlyCreationRuntimeTaskGraph(listOf(":androidApp:assembleFullDebug")))
        require(isDebugOnlyCreationRuntimeTaskGraph(listOf(
            ":androidApp:mergeFullDebugAssets",
            ":feature_creation_runtime:mergePlayDebugAssets",
        )))
        require(!isDebugOnlyCreationRuntimeTaskGraph(listOf(":androidApp:assembleFullRelease")))
        require(!isDebugOnlyCreationRuntimeTaskGraph(listOf(
            ":androidApp:assembleFullDebug",
            ":androidApp:assembleFullRelease",
        )))
        require(!isDebugOnlyCreationRuntimeTaskGraph(listOf(":androidApp:preBuild")))
    }
}

val verifyFullDownloaderRuntimeDelivery by tasks.registering(Exec::class) {
    inputs.file(downloaderRuntimeDeliveryManifest)
    inputs.file(androidAssetVerifier)
    commandLine(
        "py", "-3", androidAssetVerifier.absolutePath,
        "downloader-delivery", "--file", downloaderRuntimeDeliveryManifest.absolutePath,
    )
}

val stageFullDownloaderRuntimeDelivery by tasks.registering(Sync::class) {
    dependsOn(verifyFullDownloaderRuntimeDelivery)
    from(downloaderRuntimeDeliveryManifest) { rename { "delivery.json" } }
    into(generatedFullDownloaderRuntimeDeliveryAssets.map { it.dir("downloader-runtime") })
}

val verifyFullDownloaderLaunchers by tasks.registering(Exec::class) {
    val sources = downloaderLauncherContract.keys.map { downloaderLauncherSourceRoot.resolve(it) }
    inputs.files(sources)
    inputs.file(androidAssetVerifier)
    inputs.property(
        "launcherContract",
        downloaderLauncherContract.map { (path, identity) ->
            "$path:${identity.first}:${identity.second}"
        },
    )
    commandLine("py", "-3", androidAssetVerifier.absolutePath, "launchers")
    downloaderLauncherContract.forEach { (path, identity) ->
        args(
            "--launcher",
            "${downloaderLauncherSourceRoot.resolve(path).absolutePath}|" +
                "${identity.first}|${identity.second}",
        )
    }
}

val stageFullDownloaderLaunchers by tasks.registering(Sync::class) {
    val sources = downloaderLauncherContract.keys.map { downloaderLauncherSourceRoot.resolve(it) }
    dependsOn(verifyFullDownloaderLaunchers)
    from(sources)
    into(generatedFullDownloaderLauncherJniLibs.map { it.dir("arm64-v8a") })
}

tasks.matching {
    it.name != verifyCreationModelViewerAssets.name &&
        it.name.contains("Assets", ignoreCase = false)
}.configureEach {
    dependsOn(stageNativeRuntimeContract)
    if (name.contains("Full")) {
        dependsOn(stageComponentUpdateTrust)
    }
    dependsOn(verifyCreationModelViewerAssets)
}

tasks.matching {
    it.name.startsWith("mergeFull") && it.name.endsWith("Assets")
}.configureEach {
    dependsOn(stageFullCreationRuntimeDelivery)
    dependsOn(stageFullDownloaderRuntimeDelivery)
}

tasks.matching {
    it.name.startsWith("mergeFull") &&
        (it.name.endsWith("NativeLibs") || it.name.endsWith("JniLibFolders"))
}.configureEach {
    dependsOn(stageFullDownloaderLaunchers)
}

tasks.matching { it.name.contains("lint", ignoreCase = true) }.configureEach {
    dependsOn(stageNativeRuntimeContract)
    dependsOn(verifyCreationModelViewerAssets)
    if (name.contains("Full")) {
        dependsOn(stageComponentUpdateTrust)
        dependsOn(stageFullCreationRuntimeDelivery)
        dependsOn(stageFullDownloaderRuntimeDelivery)
    }
}
