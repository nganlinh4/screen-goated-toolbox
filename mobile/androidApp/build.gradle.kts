import groovy.json.JsonSlurper

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.androidx.baselineprofile)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.kotlin.serialization)
}

baselineProfile {
    dexLayoutOptimization = true
    automaticGenerationDuringBuild = false
    mergeIntoMain = true
}

fun extractCargoPackageVersion(cargoToml: File): String {
    var inPackageSection = false
    for (rawLine in cargoToml.readLines()) {
        val line = rawLine.trim()
        if (line.startsWith("[") && line.endsWith("]")) {
            inPackageSection = line == "[package]"
        } else if (inPackageSection && line.startsWith("version")) {
            val match = Regex("""version\s*=\s*"([^"]+)"""").find(line)
            if (match != null) {
                return match.groupValues[1]
            }
        }
    }
    error("Missing [package].version in ${cargoToml.absolutePath}")
}

val canonicalAppVersion = extractCargoPackageVersion(rootProject.projectDir.parentFile.resolve("Cargo.toml"))
val imageCreationContractFile = rootProject.projectDir.parentFile
    .resolve("parity-fixtures/image-creation-editing/state-contract.json")
val imageCreationContract = JsonSlurper().parse(imageCreationContractFile) as Map<*, *>
val imageCreatorReleaseEnabled =
    ((imageCreationContract["releaseAvailability"] as Map<*, *>)["enabled"] as Boolean)
val imageToSvgContractFile = rootProject.projectDir.parentFile
    .resolve("parity-fixtures/image-to-svg/state-contract.json")
val imageToSvgContract = JsonSlurper().parse(imageToSvgContractFile) as Map<*, *>
val imageToSvgReleaseEnabled =
    ((imageToSvgContract["releaseAvailability"] as Map<*, *>)["enabled"] as Boolean)

/** Convert semver string to an integer versionCode: "4.9.0" → 40900, "4.10.1" → 41001. */
fun semverToVersionCode(version: String): Int {
    val parts = version.split(".").map { it.toIntOrNull() ?: 0 }
    val major = parts.getOrElse(0) { 0 }
    val minor = parts.getOrElse(1) { 0 }
    val patch = parts.getOrElse(2) { 0 }
    return major * 10000 + minor * 100 + patch
}

val canonicalVersionCode = semverToVersionCode(canonicalAppVersion)

val generatedPresetOverlayAssets = layout.buildDirectory.dir("generated/presetOverlayAssets")
val generatedPresetModelCatalogSources = layout.buildDirectory.dir("generated/presetModelCatalog")
val generatedPhoneControlContract = layout.buildDirectory.dir("generated/phoneControlContract")
val generatedNativeRuntimeContractAssets =
    layout.buildDirectory.dir("generated/nativeRuntimeContractAssets")
val generatedComponentUpdateTrustAssets =
    layout.buildDirectory.dir("generated/componentUpdateTrustAssets")
val generatedModelFeedTrustAssets =
    layout.buildDirectory.dir("generated/modelFeedTrustAssets")
val generatedFullCreationRuntimeDeliveryAssets =
    layout.buildDirectory.dir("generated/fullCreationRuntimeDeliveryAssets")
val generatedFullDownloaderRuntimeDeliveryAssets =
    layout.buildDirectory.dir("generated/fullDownloaderRuntimeDeliveryAssets")
val generatedFullDownloaderLauncherJniLibs =
    layout.buildDirectory.dir("generated/fullDownloaderLauncherJniLibs")
val sharedCreationModelViewerAssets = rootProject.projectDir.parentFile
    .resolve("3d-generator-ui/viewer-dist")

val generatePresetOverlayAssets by tasks.registering(Exec::class) {
    val repoRoot = rootProject.projectDir.parentFile
    val generator = rootProject.projectDir.resolve("scripts/generate_preset_overlay_assets.py")
    val fitSource = repoRoot.resolve("src/overlay/result/markdown_view/fit.rs")
    val fitScriptSources = listOf(
        repoRoot.resolve("src/overlay/result/markdown_view/streaming/fit_impl/fit_font_script_part1.js"),
        repoRoot.resolve("src/overlay/result/markdown_view/streaming/fit_impl/fit_font_script_part2.js"),
    )
    val cssSource = repoRoot.resolve("src/overlay/result/markdown_view/css.rs")
    val buttonCanvasCssSource = repoRoot.resolve("src/overlay/result/button_canvas/css.rs")
    val buttonCanvasJsSource = repoRoot.resolve("src/overlay/result/button_canvas/js.rs")
    val buttonCanvasThemeSource = repoRoot.resolve("src/overlay/result/button_canvas/theme.rs")
    val gridJsSource = repoRoot.resolve("src/overlay/html_components/grid_js.rs")
    val recordingUiSource = repoRoot.resolve("src/overlay/recording/ui.rs")
    val iconsSource = repoRoot.resolve("src/overlay/html_components/icons.rs")
    inputs.file(fitSource)
    inputs.files(fitScriptSources)
    inputs.file(cssSource)
    inputs.file(buttonCanvasCssSource)
    inputs.file(buttonCanvasJsSource)
    inputs.file(buttonCanvasThemeSource)
    inputs.file(gridJsSource)
    inputs.file(recordingUiSource)
    inputs.file(iconsSource)
    inputs.file(generator)
    inputs.dir(projectDir.resolve("src/main/assets/preset_overlay_static"))
    outputs.dir(generatedPresetOverlayAssets)
    commandLine(
        "py", "-3", generator.absolutePath,
        "--fit-source", fitSource.absolutePath,
        "--css-source", cssSource.absolutePath,
        "--button-css-source", buttonCanvasCssSource.absolutePath,
        "--button-js-source", buttonCanvasJsSource.absolutePath,
        "--button-theme-source", buttonCanvasThemeSource.absolutePath,
        "--grid-source", gridJsSource.absolutePath,
        "--recording-source", recordingUiSource.absolutePath,
        "--icons-source", iconsSource.absolutePath,
        "--static-assets", projectDir.resolve("src/main/assets/preset_overlay_static").absolutePath,
        "--output", generatedPresetOverlayAssets.get().asFile.absolutePath,
    )
}

val generatePresetModelCatalog by tasks.registering(Exec::class) {
    val repoRoot = rootProject.projectDir.parentFile
    val manifestSource = repoRoot.resolve("catalog/model_catalog.json")
    val generator = repoRoot.resolve("scripts/generate_android_preset_model_catalog.py")
    inputs.file(manifestSource)
    inputs.file(generator)
    val outputFile = generatedPresetModelCatalogSources.get()
        .asFile
        .resolve("dev/screengoated/toolbox/mobile/preset/GeneratedPresetModelCatalogData.kt")
    outputs.file(outputFile)
    commandLine(
        "py",
        "-3",
        generator.absolutePath,
        "--manifest-source",
        manifestSource.absolutePath,
        "--preset-output",
        outputFile.absolutePath,
    )
}

val generateModelFeedTrustAssets by tasks.registering(Sync::class) {
    val publicKey = rootProject.projectDir.parentFile
        .resolve("monitoring/monitoring-p256-public-key.hex")
    inputs.file(publicKey)
    from(publicKey) {
        into("model-feed")
        rename { "public-key.hex" }
    }
    into(generatedModelFeedTrustAssets)
}

val generatePhoneControlContract by tasks.registering(Exec::class) {
    val repoRoot = rootProject.projectDir.parentFile
    val catalogSource = repoRoot.resolve("src/overlay/computer_control/phone_control_catalog.json")
    val promptSource = repoRoot.resolve("src/overlay/computer_control/uia_task/prompt_core.txt")
    val authoritySource = repoRoot.resolve("parity-fixtures/phone-control/authority-matrix.json")
    val orbContractSource = repoRoot.resolve("parity-fixtures/phone-control/orb-contract.json")
    val orbSource = repoRoot.resolve("src/overlay/computer_control/orb/orb.html")
    val generator = repoRoot.resolve("scripts/generate_android_phone_control_contract.py")
    inputs.files(
        catalogSource,
        promptSource,
        authoritySource,
        orbContractSource,
        orbSource,
        generator,
    )
    val outputRoot = generatedPhoneControlContract.get().asFile
    outputs.dir(outputRoot)
    commandLine(
        "py", "-3", generator.absolutePath,
        "--catalog-source", catalogSource.absolutePath,
        "--prompt-source", promptSource.absolutePath, "--prompt-output", outputRoot.resolve("assets/phone_control/prompt_core.txt").absolutePath,
        "--authority-source", authoritySource.absolutePath, "--authority-output", outputRoot.resolve("assets/phone_control/authority-matrix.json").absolutePath,
        "--orb-contract-source", orbContractSource.absolutePath, "--orb-contract-output", outputRoot.resolve("assets/phone_control/orb-contract.json").absolutePath,
        "--orb-source", orbSource.absolutePath, "--orb-output", outputRoot.resolve("assets/phone_control/orb.html").absolutePath,
        "--kotlin-output", outputRoot.resolve("kotlin/dev/screengoated/toolbox/mobile/phonecontrol/GeneratedPhoneControlContract.kt").absolutePath,
        "--asset-output", outputRoot.resolve("assets/phone_control/catalog.json").absolutePath,
    )
}
android {
    namespace = "dev.screengoated.toolbox.mobile"
    dynamicFeatures += setOf(
        ":feature_asr_ort",
        ":feature_asr_moonshine",
        ":feature_asr_sherpa",
        ":feature_creation_runtime",
    )
    compileSdk = 36
    // Build Tools 36 escapes Windows paths in generated AIDL comments; older output can
    // contain path fragments that javac interprets as malformed Unicode escapes.
    buildToolsVersion = "36.1.0"

    defaultConfig {
        applicationId = "dev.screengoated.toolbox.mobile"
        minSdk = 29
        targetSdk = 36
        // versionCode follows Cargo.toml semver, but can be bumped for store
        // re-uploads at the same version via -PversionCodeOverride=<int>.
        versionCode = (project.findProperty("versionCodeOverride") as String?)?.toIntOrNull()
            ?: canonicalVersionCode
        versionName = canonicalAppVersion
        buildConfigField("String", "CANONICAL_APP_VERSION", "\"$canonicalAppVersion\"")
        buildConfigField("String", "PARITY_PROFILE", "\"windows-live-translate-v2\"")
        // Overlay (float-over-other-apps) shipped on every distribution, including Play.
        buildConfigField("boolean", "OVERLAY_SUPPORTED", "true")
        buildConfigField(
            "boolean",
            "IMAGE_CREATOR_RELEASE_ENABLED",
            imageCreatorReleaseEnabled.toString(),
        )
        buildConfigField(
            "boolean",
            "IMAGE_TO_SVG_RELEASE_ENABLED",
            imageToSvgReleaseEnabled.toString(),
        )

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables.useSupportLibrary = true

        ndk {
            abiFilters += "arm64-v8a"
        }

    }

    flavorDimensions += "distribution"
    productFlavors {
        create("full") {
            dimension = "distribution"
            versionNameSuffix = "-full"
            buildConfigField("boolean", "DOWNLOADER_SUPPORTED", "true")
        }
        create("play") {
            dimension = "distribution"
            versionNameSuffix = "-play"
            // yt-dlp only stays usable by updating itself from the network, which Play's
            // Device and Network Abuse policy forbids, so the downloader ships disabled
            // here. The card stays visible and explains itself when tapped.
            buildConfigField("boolean", "DOWNLOADER_SUPPORTED", "false")
        }
    }

    signingConfigs {
        create("release") {
            val ks = rootProject.projectDir.resolve("release.keystore")
            if (ks.exists()) {
                storeFile = ks
                storePassword = "screengoated"
                keyAlias = "sgt-release"
                keyPassword = "screengoated"
            }
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            signingConfig = signingConfigs.getByName("release")
        }
        create("benchmarkRelease") {
            signingConfig = signingConfigs.getByName("debug")
        }
        create("nonMinifiedRelease") {
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
        aidl = true
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
            // Only the multi-MB zip payloads are fetched at runtime. The tiny python/ffmpeg
            // wrapper binaries must stay in the APK: from Android 10 exec() is only allowed
            // out of the real nativeLibraryDir, never app-writable storage. They are staged
            // only for Full, so Play has no downloader launchers or dependency payload.
            excludes += "**/libpython.zip.so"
            excludes += "**/libffmpeg.zip.so"
            // Native ASR runtimes are distribution-delivered by NativeLibManager.
            excludes += "**/libonnxruntime.so"
            excludes += "**/libc++_shared.so"
            excludes += "**/libmoonshine.so"
            excludes += "**/libmoonshine-jni.so"
            excludes += "**/libsherpa-onnx-jni.so"
        }
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
            // okhttp 5's logging-interceptor and jspecify both ship this stub.
            excludes += "/META-INF/versions/9/OSGI-INF/MANIFEST.MF"
            // Orphaned Bouncy Castle resources retained through libadb's transitive
            // graph even though the corresponding providers are absent after R8.
            excludes += "/org/bouncycastle/pqc/crypto/picnic/lowmcL1.bin.properties"
            excludes += "/org/bouncycastle/pqc/crypto/picnic/lowmcL3.bin.properties"
            excludes += "/org/bouncycastle/pqc/crypto/picnic/lowmcL5.bin.properties"
            excludes += "/org/bouncycastle/x509/CertPathReviewerMessages.properties"
            excludes += "/org/bouncycastle/x509/CertPathReviewerMessages_de.properties"
        }
    }

    sourceSets.named("main") {
        // Saved models remain viewable after the removable creation runtime is uninstalled.
        assets.directories.add(sharedCreationModelViewerAssets.absolutePath)
        assets.directories.add(generatedPresetOverlayAssets.get().asFile.absolutePath)
        kotlin.directories.add(generatedPresetModelCatalogSources.get().asFile.absolutePath)
        assets.directories.add(generatedPhoneControlContract.get().dir("assets").asFile.absolutePath)
        kotlin.directories.add(generatedPhoneControlContract.get().dir("kotlin").asFile.absolutePath)
        assets.directories.add(generatedNativeRuntimeContractAssets.get().asFile.absolutePath)
        assets.directories.add(generatedModelFeedTrustAssets.get().asFile.absolutePath)
    }
    sourceSets.named("full") {
        assets.directories.add(rootProject.projectDir.resolve("native/sherpa-runtime/assets").absolutePath)
        assets.directories.add(rootProject.projectDir.resolve("native/ort-runtime/assets").absolutePath)
        assets.directories.add(rootProject.projectDir.resolve("native/moonshine-runtime/assets").absolutePath)
        assets.directories.add(generatedFullCreationRuntimeDeliveryAssets.get().asFile.absolutePath)
        assets.directories.add(generatedFullDownloaderRuntimeDeliveryAssets.get().asFile.absolutePath)
        assets.directories.add(generatedComponentUpdateTrustAssets.get().asFile.absolutePath)
        jniLibs.directories.add(generatedFullDownloaderLauncherJniLibs.get().asFile.absolutePath)
    }
    sourceSets.maybeCreate("testFullDebug").kotlin.directories.add(file("src/testDebug/java").absolutePath)
    sourceSets.maybeCreate("testPlayDebug").kotlin.directories.add(file("src/testDebug/java").absolutePath)
}

tasks.matching {
    it.name != generatePresetOverlayAssets.name &&
        it.name != generateModelFeedTrustAssets.name &&
        it.name != "verifyCreationModelViewerAssets" &&
        it.name.contains("Assets", ignoreCase = false)
}.configureEach {
    dependsOn(generatePresetOverlayAssets)
    dependsOn(generateModelFeedTrustAssets)
}

tasks.matching {
    it.name != generatePresetModelCatalog.name &&
        (it.name.contains("Kotlin", ignoreCase = false) || it.name.contains("Java", ignoreCase = false))
}.configureEach {
    dependsOn(generatePresetModelCatalog)
}

tasks.matching {
    it.name != generatePhoneControlContract.name &&
        (it.name.contains("Kotlin") || it.name.contains("Java") ||
            it.name.contains("Assets") || it.name.contains("lint", ignoreCase = true))
}.configureEach {
    dependsOn(generatePhoneControlContract)
}

tasks.matching {
    it.name.contains("lint", ignoreCase = true)
}.configureEach {
    dependsOn(generatePresetOverlayAssets)
    dependsOn(generatePresetModelCatalog)
    dependsOn(generateModelFeedTrustAssets)
}

dependencies {
    implementation(project(":shared"))
    "baselineProfile"(project(":baselineprofile"))

    implementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(platform(libs.androidx.compose.bom))

    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.fragment)
    implementation(libs.androidx.compose.foundation)
    // material-icons-extended removed — replaced by Material Symbols vector drawables (res/drawable/ms_*.xml)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.graphics.shapes)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.core.splashscreen)
    implementation(libs.androidx.profileinstaller)
    implementation(libs.androidx.browser)
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
    implementation(libs.jsoup)
    implementation(libs.moonshine.voice)
    implementation(files("libs/sherpa-onnx-static-1.12.35.aar"))
    implementation(libs.androidx.media3.session)
    implementation(libs.androidx.media3.common)
    implementation(libs.commonmark)
    implementation(libs.commonmark.ext.gfm.tables)
    implementation(libs.commonmark.ext.gfm.strikethrough)
    implementation(libs.commonmark.ext.task.list.items)
    implementation(libs.shizuku.api)
    implementation(libs.shizuku.provider)
    implementation(libs.libadb.android)
    // Google Play In-App Updates (used by the `play` flavor; no-ops on sideload installs).
    implementation(libs.play.app.update.ktx)
    implementation(libs.play.feature.delivery)
    implementation(libs.play.feature.delivery.ktx)

    debugImplementation(libs.androidx.compose.ui.test.manifest)
    debugImplementation(libs.androidx.compose.ui.tooling)

    testImplementation(libs.junit4)
    testImplementation(libs.kotlinx.coroutines.test)
    // Real org.json for JVM unit tests (the android.jar stub throws "not mocked"),
    // so parity tests can exercise the org.json-based S2S setup-payload builder.
    testImplementation("org.json:json:20240303")
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.espresso.core)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.uiautomator)
}

apply(from = file("gradle/runtime-delivery.gradle.kts"))
apply(from = file("gradle/play-compliance.gradle.kts"))
