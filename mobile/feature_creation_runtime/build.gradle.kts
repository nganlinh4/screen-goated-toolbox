import java.io.FileOutputStream
import java.net.URI
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest

plugins {
    alias(libs.plugins.android.dynamic.feature)
}

val runtimeUrl =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/sgt-creation-runtime-android.aar"
val runtimeBytes = 398_811L
val runtimeSha256 = "77b945651e88f0cd39ee0bdf6f04a77c8ce6be24d54c2a7333fbdf5b803a457c"
val generatedRuntime = layout.buildDirectory.file("generated/runtime/sgt-creation-runtime-android.aar")
val localCandidates = listOf(
    rootProject.projectDir.parentFile.resolve(
        "local-runtime-bundles/sgt_creation_runtime/sgt-creation-runtime-android.aar",
    ),
    rootProject.projectDir.parentFile.resolve(
        "native/sgt_3d_generator_runtime/android-runtime/dist/android/sgt-creation-runtime-android.aar",
    ),
)

fun validRuntime(file: File): Boolean {
    if (!file.isFile || file.length() != runtimeBytes) return false
    val digest = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { input ->
        val buffer = ByteArray(128 * 1024)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
    }
    return digest.digest().joinToString("") { "%02x".format(it) } == runtimeSha256
}

val prepareCreationRuntime by tasks.registering {
    inputs.property("runtimeUrl", runtimeUrl)
    inputs.property("runtimeSha256", runtimeSha256)
    localCandidates.filter(File::isFile).forEach { inputs.file(it) }
    outputs.file(generatedRuntime)
    outputs.upToDateWhen { validRuntime(generatedRuntime.get().asFile) }
    doLast {
        val output = generatedRuntime.get().asFile
        output.parentFile.mkdirs()
        localCandidates.firstOrNull(::validRuntime)?.let { local ->
            local.copyTo(output, overwrite = true)
            return@doLast
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
                    check(written <= runtimeBytes) { "Creation runtime download exceeds contract" }
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

android {
    namespace = "dev.screengoated.toolbox.mobile.feature.creation.runtime"
    compileSdk = 36
    defaultConfig { minSdk = 29 }
    flavorDimensions += "distribution"
    productFlavors {
        create("full") { dimension = "distribution" }
        create("play") { dimension = "distribution" }
    }
}

tasks.named("preBuild").configure { dependsOn(prepareCreationRuntime) }

dependencies {
    implementation(project(":androidApp"))
    implementation(files(generatedRuntime))
}
