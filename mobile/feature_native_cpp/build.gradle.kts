plugins {
    alias(libs.plugins.android.dynamic.feature)
    alias(libs.plugins.kotlin.android)
}

val generatedJni = layout.buildDirectory.dir("generated/jniLibs")
val prepareNativePayload by tasks.registering(Sync::class) {
    dependsOn(rootProject.tasks.named("verifyNativeRuntimeArchives"))
    from(zipTree(project(":androidApp").projectDir.resolve("libs/ort-runtime.zip"))) {
        include("libc++_shared.so")
    }
    into(generatedJni.map { it.dir("arm64-v8a") })
    outputs.upToDateWhen { false }
}

android {
    namespace = "dev.screengoated.toolbox.mobile.feature.nativecpp"
    compileSdk = 36
    defaultConfig { minSdk = 29 }
    flavorDimensions += "distribution"
    productFlavors {
        create("full") { dimension = "distribution" }
        create("play") { dimension = "distribution" }
    }
    sourceSets.named("main") { jniLibs.srcDir(generatedJni) }
    packaging {
        // libc++_shared is linked by the exec'd Python/FFmpeg binaries, so extract
        // it to the exec-allowed native lib dir rather than leaving it compressed.
        jniLibs {
            useLegacyPackaging = true
            keepDebugSymbols += "**/libc++_shared.so"
        }
    }
}

tasks.named("preBuild").configure { dependsOn(prepareNativePayload) }

dependencies {
    implementation(project(":androidApp"))
}
