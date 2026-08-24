plugins {
    alias(libs.plugins.android.dynamic.feature)
}

val generatedJni = layout.buildDirectory.dir("generated/jniLibs")
val prepareNativePayload by tasks.registering(Sync::class) {
    dependsOn(rootProject.tasks.named("verifyNativeRuntimeArchives"))
    from(zipTree(project(":androidApp").projectDir.resolve("libs/ort-runtime.zip"))) {
        include("libonnxruntime.so", "libonnxruntime_real.so")
    }
    into(generatedJni.map { it.dir("arm64-v8a") })
    outputs.upToDateWhen { false }
}

android {
    enableKotlin = false
    namespace = "dev.screengoated.toolbox.mobile.feature.asr.ort"
    compileSdk = 36
    defaultConfig { minSdk = 29 }
    flavorDimensions += "distribution"
    productFlavors {
        create("full") { dimension = "distribution" }
        create("play") { dimension = "distribution" }
    }
    sourceSets.named("main") {
        jniLibs.directories.add(generatedJni.get().asFile.absolutePath)
        assets.directories.add(rootProject.projectDir.resolve("native/ort-runtime/assets").absolutePath)
    }
    packaging.jniLibs.keepDebugSymbols += setOf(
        "**/libonnxruntime.so",
        "**/libonnxruntime_real.so",
    )
}

tasks.named("preBuild").configure { dependsOn(prepareNativePayload) }

dependencies {
    implementation(project(":androidApp"))
}
