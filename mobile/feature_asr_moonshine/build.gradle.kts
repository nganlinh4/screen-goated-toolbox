plugins {
    alias(libs.plugins.android.dynamic.feature)
    alias(libs.plugins.kotlin.android)
}

val generatedJni = layout.buildDirectory.dir("generated/jniLibs")
val prepareNativePayload by tasks.registering(Sync::class) {
    dependsOn(rootProject.tasks.named("verifyNativeRuntimeArchives"))
    from(zipTree(project(":androidApp").projectDir.resolve("libs/moonshine-runtime.zip"))) {
        include("libmoonshine.so", "libmoonshine-jni.so")
    }
    into(generatedJni.map { it.dir("arm64-v8a") })
    outputs.upToDateWhen { false }
}

android {
    namespace = "dev.screengoated.toolbox.mobile.feature.asr.moonshine"
    compileSdk = 36
    defaultConfig { minSdk = 29 }
    flavorDimensions += "distribution"
    productFlavors {
        create("full") { dimension = "distribution" }
        create("play") { dimension = "distribution" }
    }
    sourceSets.named("main") { jniLibs.srcDir(generatedJni) }
    packaging.jniLibs.keepDebugSymbols += setOf(
        "**/libmoonshine.so",
        "**/libmoonshine-jni.so",
    )
}

tasks.named("preBuild").configure { dependsOn(prepareNativePayload) }

dependencies {
    implementation(project(":androidApp"))
}
