import java.io.FileOutputStream
import java.net.URI
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest

plugins {
    alias(libs.plugins.android.dynamic.feature)
    alias(libs.plugins.kotlin.android)
}

val generatedJni = layout.buildDirectory.dir("generated/jniLibs")
val generatedDetectorAssets = layout.buildDirectory.dir("generated/detectorAssets")
val detectorModelUrl =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/ui-detr-1.onnx"
val detectorModelBytes = 131_216_489L
val detectorModelSha256 =
    "1892092320cd55fd182c6afd76ae5bb0fb9695f5fcdf0ba875c1f68d49792ff4"
val prepareNativePayload by tasks.registering(Sync::class) {
    dependsOn(rootProject.tasks.named("verifyNativeRuntimeArchives"))
    from(zipTree(project(":androidApp").projectDir.resolve("libs/ort-runtime.zip"))) {
        include("libonnxruntime.so", "libonnxruntime_real.so")
    }
    into(generatedJni.map { it.dir("arm64-v8a") })
    outputs.upToDateWhen { false }
}

val detectorModelFile = generatedDetectorAssets.map {
    it.file("ui_detector/ui-detr-1.onnx")
}
val localDetectorCandidates = listOfNotNull(
    project(":androidApp").projectDir.resolve("libs/ui-detr-1.onnx"),
    System.getenv("APPDATA")?.let { File(it, "screen-goated-toolbox/models/ui-detector/ui-detr-1.onnx") },
).filter(File::isFile)

fun validDetectorModel(file: File): Boolean {
    if (!file.isFile || file.length() != detectorModelBytes) return false
    val digest = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { input ->
        val buffer = ByteArray(1024 * 1024)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
    }
    return digest.digest().joinToString("") { byte -> "%02x".format(byte) } ==
        detectorModelSha256
}

val prepareDetectorModel by tasks.registering {
    inputs.property("modelUrl", detectorModelUrl)
    inputs.property("modelSha256", detectorModelSha256)
    localDetectorCandidates.forEach { inputs.file(it) }
    outputs.file(detectorModelFile)
    outputs.upToDateWhen { validDetectorModel(detectorModelFile.get().asFile) }
    doLast {
        val output = detectorModelFile.get().asFile
        output.parentFile.mkdirs()
        localDetectorCandidates.firstOrNull(::validDetectorModel)?.let { local ->
            local.copyTo(output, overwrite = true)
            return@doLast
        }
        val partial = File(output.parentFile, "${output.name}.part")
        partial.delete()
        val connection = URI(detectorModelUrl).toURL().openConnection().apply {
            connectTimeout = 30_000
            readTimeout = 120_000
        }
        connection.getInputStream().use { input ->
            FileOutputStream(partial).use { target ->
                val buffer = ByteArray(1024 * 1024)
                var written = 0L
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    written += read
                    check(written <= detectorModelBytes) { "UI detector download exceeds contract" }
                    target.write(buffer, 0, read)
                }
                target.fd.sync()
            }
        }
        check(validDetectorModel(partial)) { "Downloaded UI detector failed size/hash validation" }
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

android {
    namespace = "dev.screengoated.toolbox.mobile.feature.asr.ort"
    compileSdk = 36
    defaultConfig { minSdk = 29 }
    flavorDimensions += "distribution"
    productFlavors {
        create("full") { dimension = "distribution" }
        create("play") { dimension = "distribution" }
    }
    sourceSets.named("main") { jniLibs.srcDir(generatedJni) }
    sourceSets.named("play") { assets.srcDir(generatedDetectorAssets) }
    packaging.jniLibs.keepDebugSymbols += setOf(
        "**/libonnxruntime.so",
        "**/libonnxruntime_real.so",
    )
}

tasks.named("preBuild").configure { dependsOn(prepareNativePayload) }
tasks.matching { it.name.startsWith("prePlay") && it.name.endsWith("Build") }
    .configureEach { dependsOn(prepareDetectorModel) }

dependencies {
    implementation(project(":androidApp"))
}
