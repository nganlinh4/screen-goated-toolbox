import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.serialization)
}

fun extractWindowsRawString(source: String, marker: String): String {
    val markerIndex = source.indexOf(marker)
    require(markerIndex >= 0) { "Missing marker: $marker" }
    val start = source.indexOf("r#\"", markerIndex)
    require(start >= 0) { "Missing raw string start for: $marker" }
    val contentStart = start + 3
    val end = source.indexOf("\"#;", contentStart)
    require(end >= 0) { "Missing raw string end for: $marker" }
    return source.substring(contentStart, end)
}

val generatedPresetOverlayAssets = layout.buildDirectory.dir("generated/presetOverlayAssets")
val generatePresetOverlayAssets by tasks.registering {
    val repoRoot = rootProject.projectDir.parentFile
    val fitSource = repoRoot.resolve("src/overlay/result/markdown_view/streaming/fit_impl.rs")
    val cssSource = repoRoot.resolve("src/overlay/result/markdown_view/css.rs")
    inputs.file(fitSource)
    inputs.file(cssSource)
    outputs.dir(generatedPresetOverlayAssets)

    doLast {
        val outputDir = generatedPresetOverlayAssets.get().asFile.resolve("preset_overlay")
        outputDir.mkdirs()

        val fitScript = extractWindowsRawString(
            fitSource.readText(),
            "const FIT_FONT_SCRIPT: &str = r#\"",
        )
        outputDir.resolve("windows_markdown_fit.js").writeText(
            """
            window.runWindowsMarkdownFit = function(streamingMode, phase) {
                const source = ${groovy.json.JsonOutput.toJson(fitScript)};
                const resolved = source
                    .replace(/__FIT_PHASE__/g, phase || "mobile_markdown_fit")
                    .replace(/__STREAMING_MODE__/g, streamingMode ? "true" : "false");
                return window.eval(resolved);
            };
            """.trimIndent(),
        )

        val markdownCss = extractWindowsRawString(
            cssSource.readText(),
            "pub const MARKDOWN_CSS: &str = r#\"",
        )
        outputDir.resolve("windows_markdown.css").writeText(markdownCss)
    }
}

android {
    namespace = "dev.screengoated.toolbox.mobile"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.screengoated.toolbox.mobile"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        buildConfigField("String", "PARITY_PROFILE", "\"windows-live-translate-v2\"")

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables.useSupportLibrary = true
    }

    flavorDimensions += "distribution"
    productFlavors {
        create("full") {
            dimension = "distribution"
            versionNameSuffix = "-full"
            buildConfigField("boolean", "OVERLAY_SUPPORTED", "true")
        }
        create("play") {
            dimension = "distribution"
            versionNameSuffix = "-play"
            buildConfigField("boolean", "OVERLAY_SUPPORTED", "false")
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlin {
        compilerOptions {
            jvmTarget.set(JvmTarget.JVM_17)
        }
        jvmToolchain(17)
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }

    sourceSets.named("main") {
        assets.srcDir(generatedPresetOverlayAssets)
    }
}

tasks.matching {
    it.name != generatePresetOverlayAssets.name && it.name.contains("Assets", ignoreCase = false)
}.configureEach {
    dependsOn(generatePresetOverlayAssets)
}

dependencies {
    implementation(project(":shared"))

    implementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(platform(libs.androidx.compose.bom))

    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.compose.material.icons.extended)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.graphics.shapes)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.service)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.androidx.security.crypto.ktx)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.okhttp)
    implementation(libs.okhttp.logging)
    implementation(libs.onnxruntime.android)
    implementation("org.commonmark:commonmark:0.24.0")
    implementation("io.github.junkfood02.youtubedl-android:library:0.18.1")
    implementation("io.github.junkfood02.youtubedl-android:ffmpeg:0.18.1")

    debugImplementation(libs.androidx.compose.ui.test.manifest)
    debugImplementation(libs.androidx.compose.ui.tooling)

    testImplementation(libs.junit4)
    testImplementation(libs.kotlinx.coroutines.test)
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.espresso.core)
    androidTestImplementation(libs.androidx.junit)
}
