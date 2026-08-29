import groovy.json.JsonSlurper

plugins {
    alias(libs.plugins.android.dynamic.feature)
}

data class RuntimeArtifactContract(
    val asset: String,
    val sizeBytes: Long,
    val sha256: String,
    val downloadUrl: String?,
)

fun Map<*, *>.requiredMap(name: String): Map<*, *> =
    this[name] as? Map<*, *> ?: error("Creation runtime manifest is missing $name")

fun Map<*, *>.requiredString(name: String): String =
    (this[name] as? String)?.takeIf(String::isNotBlank)
        ?: error("Creation runtime manifest is missing $name")

fun Map<*, *>.requiredLong(name: String): Long =
    (this[name] as? Number)?.toLong()?.takeIf { it > 0L }
        ?: error("Creation runtime manifest has invalid $name")

val repoRoot = rootProject.projectDir.parentFile
evaluationDependsOn(":androidApp")
val manifestFile = File(
    rootProject.extensions.extraProperties.get("sgtCreationRuntimeDeliveryManifest") as? String
        ?: error("Android app did not select a creation runtime delivery contract"),
)
val runtimeDeliveryChannel =
    rootProject.extensions.extraProperties.get("sgtCreationRuntimeDeliveryChannel") as? String
        ?: error("Android app did not select a creation runtime delivery channel")
require(manifestFile.isFile) {
    "Selected creation runtime delivery contract is required: $manifestFile"
}
val runtimeContract = manifestFile.let { source ->
    @Suppress("UNCHECKED_CAST")
    val root = JsonSlurper().parse(source) as Map<*, *>
    val cargoVersion = Regex("(?m)^version\\s*=\\s*\"([^\"]+)\"")
        .find(repoRoot.resolve("Cargo.toml").readText())
        ?.groupValues
        ?.get(1)
        ?: error("Root Cargo package version is missing")
    require(root.requiredString("hostVersion") == cargoVersion) {
        "Creation runtime manifest targets another app version"
    }
    val play = root.requiredMap("android").requiredMap("play")
    RuntimeArtifactContract(
        asset = (play["asset"] ?: play["file"]) as? String
            ?: error("Creation runtime manifest is missing play asset"),
        sizeBytes = play.requiredLong("sizeBytes"),
        sha256 = play.requiredString("sha256"),
        downloadUrl = (play["downloadUrl"] as? String)?.takeIf { it.isNotBlank() },
    ).also { contract ->
        require(
            contract.asset.isNotEmpty() &&
                '/' !in contract.asset &&
                '\\' !in contract.asset &&
                contract.asset !in setOf(".", ".."),
        ) { "Creation runtime asset must be a file name" }
        require(
            contract.sha256.length == 64 &&
                contract.sha256.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' },
        ) { "Creation runtime manifest has invalid SHA-256" }
        val expectedAsset = "sgt-creation-runtime-android-${contract.sha256.take(16)}.aar"
        require(contract.asset == expectedAsset) {
            "Creation runtime Play asset is not content-addressed"
        }
        val expectedTag = when (runtimeDeliveryChannel) {
            "Production" -> "sgt-runtime-bundles"
            "Staging" -> "sgt-runtime-staging"
            else -> error("Unsupported creation runtime delivery channel $runtimeDeliveryChannel")
        }
        require(
            contract.downloadUrl ==
                "https://github.com/nganlinh4/screen-goated-toolbox/releases/" +
                "download/$expectedTag/${contract.asset}",
        ) { "Creation runtime Play URL does not use the selected delivery tag" }
    }
}

val generatedRuntime =
    layout.buildDirectory.file("generated/runtime/${runtimeContract.asset}")
val generatedDeliveryAssets = layout.buildDirectory.dir("generated/runtimeDeliveryAssets")

val prepareCreationRuntime by tasks.registering(Exec::class) {
    val installer = rootProject.projectDir.resolve("scripts/prepare_creation_runtime.py")
    val output = generatedRuntime.get().asFile
    inputs.file(manifestFile)
    inputs.file(installer)
    inputs.property("runtimeDelivery", runtimeContract.downloadUrl)
    inputs.property("runtimeSha256", runtimeContract.sha256)
    outputs.file(output)
    commandLine(
        "py",
        "-3",
        installer.absolutePath,
        "--url",
        requireNotNull(runtimeContract.downloadUrl),
        "--output",
        output.absolutePath,
        "--byte-count",
        runtimeContract.sizeBytes.toString(),
        "--sha256",
        runtimeContract.sha256,
    )
}

val stageCreationRuntimeDelivery by tasks.registering(Sync::class) {
    inputs.file(manifestFile)
    inputs.property("creationRuntimeDeliveryChannel", runtimeDeliveryChannel)
    from(manifestFile) { rename { "delivery.json" } }
    into(generatedDeliveryAssets.map { it.dir("creation-runtime") })
}

val testDebugCreationRuntimeDeliverySelectionParity by tasks.registering {
    group = "verification"
    description = "Verifies Full and Play consume the same selected creation contract."
    dependsOn(":androidApp:testDebugCreationRuntimeDeliverySelection")
    inputs.file(manifestFile)
    inputs.property("creationRuntimeDeliveryChannel", runtimeDeliveryChannel)
    doLast {
        val selectedByBase = File(
            rootProject.extensions.extraProperties.get(
                "sgtCreationRuntimeDeliveryManifest",
            ) as String,
        )
        require(manifestFile.canonicalFile == selectedByBase.canonicalFile) {
            "Full and Play creation runtime delivery contracts differ"
        }
        require(runtimeContract.downloadUrl?.contains(
            when (runtimeDeliveryChannel) {
                "Production" -> "/sgt-runtime-bundles/"
                "Staging" -> "/sgt-runtime-staging/"
                else -> error("Unsupported creation runtime delivery channel")
            },
        ) == true) { "Play creation runtime URL and selected channel differ" }
    }
}

android {
    enableKotlin = false
    namespace = "dev.screengoated.toolbox.mobile.feature.creation.runtime"
    compileSdk = 36
    defaultConfig { minSdk = 29 }
    flavorDimensions += "distribution"
    productFlavors {
        create("full") { dimension = "distribution" }
        create("play") { dimension = "distribution" }
    }
    sourceSets.named("main") {
        assets.directories.add(generatedDeliveryAssets.get().asFile.absolutePath)
    }
}

tasks.named("preBuild").configure {
    dependsOn(prepareCreationRuntime)
    dependsOn(stageCreationRuntimeDelivery)
}

dependencies {
    implementation(project(":androidApp"))
    implementation(files(generatedRuntime))
}
