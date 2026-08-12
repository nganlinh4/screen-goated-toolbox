import groovy.json.JsonSlurper
import org.gradle.api.tasks.Sync
import java.security.MessageDigest

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
val creationRuntimeDeliveryManifest = rootProject.projectDir.parentFile.resolve(
    "component-delivery/creation-runtime-v1.json",
)
val creationRuntimeHostVersion = Regex("(?m)^version\\s*=\\s*\"([^\"]+)\"")
    .find(rootProject.projectDir.parentFile.resolve("Cargo.toml").readText())
    ?.groupValues
    ?.get(1)
    ?: error("Root Cargo package version is missing")
val downloaderRuntimeDeliveryManifest = projectDir.resolve("delivery/downloader-runtime.json")
val downloaderLauncherSourceRoot = rootProject.projectDir.resolve("../../youtubedl-android")
val downloaderLauncherContract = linkedMapOf(
    "library/src/main/jniLibs/arm64-v8a/libpython.so" to Pair(
        5_688L,
        "1925b8bac20eb935888e86e28e121d3875a5080a823c8f0f6b669142900bdaa8",
    ),
    "ffmpeg/src/main/jniLibs/arm64-v8a/libffmpeg.so" to Pair(
        5_488L,
        "d978afc3e7354ac00ebe947b5388391bc00e5a66a0ce075dba4b359fe6abaf23",
    ),
    "ffmpeg/src/main/jniLibs/arm64-v8a/libffprobe.so" to Pair(
        5_560L,
        "82e407f0fc95152bc1d4a713f6b2b680fe8678bf5b929a3b35d765fdf36cb8e4",
    ),
)

fun File.sha256(): String {
    val digest = MessageDigest.getInstance("SHA-256").digest(readBytes())
    return digest.joinToString(separator = "") { "%02x".format(it) }
}

val stageNativeRuntimeContract by tasks.registering(Sync::class) {
    dependsOn(rootProject.tasks.named("verifyNativeRuntimeArchives"))
    from(nativeRuntimeContractSource) { rename { "contract.json" } }
    into(generatedNativeRuntimeContractAssets.map { it.dir("native-runtime") })
}

val stageComponentUpdateTrust by tasks.registering(Sync::class) {
    inputs.file(componentUpdatePublicKey)
    doFirst {
        require(componentUpdatePublicKey.isFile) {
            "Tracked component-update public key is required: $componentUpdatePublicKey"
        }
        require(componentUpdatePublicKey.readText().trim().matches(Regex("04[0-9a-f]{128}"))) {
            "Component-update public key must be an uncompressed P-256 point"
        }
    }
    from(componentUpdatePublicKey) { rename { "public-key.hex" } }
    into(generatedComponentUpdateTrustAssets.map { it.dir("component-update") })
}

val verifyCreationModelViewerAssets by tasks.registering {
    val expected = setOf(
        "creation_model_viewer/index.html",
        "creation_model_viewer/assets/viewer.css",
        "creation_model_viewer/assets/viewer.js",
    )
    inputs.dir(sharedCreationModelViewerAssets)
    doLast {
        require(sharedCreationModelViewerAssets.isDirectory) {
            "Shared creation viewer build is missing: $sharedCreationModelViewerAssets"
        }
        val actual = sharedCreationModelViewerAssets.walkTopDown()
            .filter { it.isFile }
            .map { it.relativeTo(sharedCreationModelViewerAssets).invariantSeparatorsPath }
            .toSet()
        require(actual == expected) {
            "Shared creation viewer must contain exactly $expected, found $actual"
        }
        val document = sharedCreationModelViewerAssets
            .resolve("creation_model_viewer/index.html")
            .readText()
        require("data-viewer-version=\"1\"" in document) {
            "Shared creation viewer document version is missing"
        }
        require("default-src 'none'" in document && "connect-src 'self'" in document) {
            "Shared creation viewer CSP must deny external resources"
        }
    }
}

val stageFullCreationRuntimeDelivery by tasks.registering(Sync::class) {
    inputs.file(creationRuntimeDeliveryManifest)
    doFirst {
        require(creationRuntimeDeliveryManifest.isFile) {
            "Tracked creation runtime delivery contract is required: " +
                creationRuntimeDeliveryManifest
        }
        val root = JsonSlurper().parse(creationRuntimeDeliveryManifest) as Map<*, *>
        require(root["hostVersion"] == creationRuntimeHostVersion) {
            "Creation runtime manifest targets another app version"
        }
    }
    from(creationRuntimeDeliveryManifest) { rename { "delivery.json" } }
    into(generatedFullCreationRuntimeDeliveryAssets.map { it.dir("creation-runtime") })
}

val stageFullDownloaderRuntimeDelivery by tasks.registering(Sync::class) {
    inputs.file(downloaderRuntimeDeliveryManifest)
    doFirst {
        require(downloaderRuntimeDeliveryManifest.isFile) {
            "Full downloader delivery manifest is required: $downloaderRuntimeDeliveryManifest"
        }
        val root = JsonSlurper().parse(downloaderRuntimeDeliveryManifest) as Map<*, *>
        require((root["schemaVersion"] as? Number)?.toInt() == 1) {
            "Unsupported downloader delivery schema"
        }
        require(root["abi"] == "arm64-v8a") { "Downloader delivery must target arm64-v8a" }
        val version = (root["version"] as? String)?.takeIf(String::isNotBlank)
            ?: error("Downloader delivery version is missing")
        val artifacts = root["artifacts"] as? List<*>
            ?: error("Downloader delivery artifacts are missing")
        val contracts = artifacts.map { it as? Map<*, *> ?: error("Invalid downloader artifact") }
        require(contracts.map { it["role"] }.toSet() == setOf("yt_dlp", "python", "ffmpeg")) {
            "Downloader delivery roles must be yt_dlp, python, and ffmpeg"
        }
        require(contracts.size == 3) { "Downloader delivery repeats an artifact" }
        contracts.forEach { contract ->
            val role = contract["role"] as String
            val asset = (contract["asset"] as? String)?.takeIf {
                it.isNotBlank() && '/' !in it && '\\' !in it
            } ?: error("Invalid downloader asset for $role")
            val url = contract["downloadUrl"] as? String
                ?: error("Downloader URL is missing for $role")
            val bytes = (contract["sizeBytes"] as? Number)?.toLong() ?: 0L
            val sha256 = contract["sha256"] as? String ?: ""
            require(bytes > 0L && sha256.matches(Regex("[0-9a-f]{64}"))) {
                "Invalid downloader identity for $role"
            }
            require(url.endsWith("/$asset")) { "Downloader asset URL differs for $role" }
            if (role == "yt_dlp") {
                require(url.matches(Regex(
                    "https://github\\.com/yt-dlp/yt-dlp/releases/download/[0-9.]+/yt-dlp",
                ))) { "yt-dlp must use an immutable official release URL" }
                require(version.startsWith(url.substringAfter("/download/").substringBefore('/'))) {
                    "yt-dlp version and delivery version differ"
                }
            } else {
                require(url.startsWith(
                    "https://github.com/nganlinh4/screen-goated-toolbox/releases/" +
                        "download/sgt-runtime-bundles/sgt-downloader-",
                )) { "$role must use a uniquely named sgt-runtime-bundles asset" }
                require(asset.contains(sha256.take(12))) {
                    "$role asset must include its SHA-256 prefix"
                }
                require((contract["entryCount"] as? Number)?.toInt()?.let { it > 0 } == true &&
                    (contract["uncompressedBytes"] as? Number)?.toLong()?.let { it > 0L } == true
                ) { "$role extraction contract is incomplete" }
                require((contract["requiredPaths"] as? List<*>)?.isNotEmpty() == true) {
                    "$role required paths are missing"
                }
            }
        }
    }
    from(downloaderRuntimeDeliveryManifest) { rename { "delivery.json" } }
    into(generatedFullDownloaderRuntimeDeliveryAssets.map { it.dir("downloader-runtime") })
}

val stageFullDownloaderLaunchers by tasks.registering(Sync::class) {
    val sources = downloaderLauncherContract.keys.map { downloaderLauncherSourceRoot.resolve(it) }
    inputs.files(sources)
    inputs.property(
        "launcherContract",
        downloaderLauncherContract.map { (path, identity) ->
            "$path:${identity.first}:${identity.second}"
        },
    )
    doFirst {
        downloaderLauncherContract.forEach { (path, identity) ->
            val source = downloaderLauncherSourceRoot.resolve(path)
            require(source.isFile) { "Full downloader launcher is missing: $source" }
            require(source.length() == identity.first) {
                "Full downloader launcher size mismatch: $source"
            }
            require(source.sha256() == identity.second) {
                "Full downloader launcher SHA-256 mismatch: $source"
            }
        }
    }
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
