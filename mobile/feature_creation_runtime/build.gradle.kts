import groovy.json.JsonSlurper
import java.io.FileOutputStream
import java.net.URI
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest

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

fun validRuntime(file: File): Boolean {
    if (!file.isFile || file.length() != runtimeContract.sizeBytes) return false
    val digest = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { input ->
        val buffer = ByteArray(128 * 1024)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
    }
    return digest.digest().joinToString("") { "%02x".format(it) } == runtimeContract.sha256
}

val prepareCreationRuntime by tasks.registering {
    inputs.file(manifestFile)
    inputs.property("runtimeDelivery", runtimeContract.downloadUrl)
    inputs.property("runtimeSha256", runtimeContract.sha256)
    outputs.file(generatedRuntime)
    outputs.upToDateWhen { validRuntime(generatedRuntime.get().asFile) }
    doLast {
        val output = generatedRuntime.get().asFile
        output.parentFile.mkdirs()
        val runtimeUrl = requireNotNull(runtimeContract.downloadUrl) {
            "Creation runtime download contract is missing its immutable URL"
        }
        val partial = File(output.parentFile, "${output.name}.part")
        partial.delete()
        val connection = URI(runtimeUrl).toURL().openConnection().apply {
            connectTimeout = 30_000
            readTimeout = 120_000
        }
        connection.getInputStream().use { input ->
            FileOutputStream(partial).use { target ->
                val buffer = ByteArray(128 * 1024)
                var written = 0L
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    written += read
                    check(written <= runtimeContract.sizeBytes) {
                        "Creation runtime download exceeds contract"
                    }
                    target.write(buffer, 0, read)
                }
                target.fd.sync()
            }
        }
        check(validRuntime(partial)) { "Downloaded creation runtime failed validation" }
        runCatching {
            Files.move(
                partial.toPath(),
                output.toPath(),
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
        }.getOrElse {
            Files.move(partial.toPath(), output.toPath(), StandardCopyOption.REPLACE_EXISTING)
        }
    }
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
    namespace = "dev.screengoated.toolbox.mobile.feature.creation.runtime"
    compileSdk = 36
    defaultConfig { minSdk = 29 }
    flavorDimensions += "distribution"
    productFlavors {
        create("full") { dimension = "distribution" }
        create("play") { dimension = "distribution" }
    }
    sourceSets.named("main") {
        assets.srcDir(generatedDeliveryAssets)
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
