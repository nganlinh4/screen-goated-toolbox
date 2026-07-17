import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.io.InputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipFile

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
    val end = source.indexOf("\"#", contentStart)
    require(end >= 0) { "Missing raw string end for: $marker" }
    return source.substring(contentStart, end)
}

fun extractRustConcatIncludeStrings(sourceFile: File, marker: String): String {
    val source = sourceFile.readText()
    val markerIndex = source.indexOf(marker)
    require(markerIndex >= 0) { "Missing marker: $marker" }
    val start = source.indexOf("concat!(", markerIndex)
    require(start >= 0) { "Missing concat start for: $marker" }
    val end = source.indexOf(");", start)
    require(end >= 0) { "Missing concat end for: $marker" }
    val body = source.substring(start, end)
    val includePaths = Regex("""include_str!\("([^"]+)"\)""")
        .findAll(body)
        .map { it.groupValues[1] }
        .toList()
    require(includePaths.isNotEmpty()) { "Missing include_str entries for: $marker" }
    return includePaths.joinToString(separator = "") { relativePath ->
        sourceFile.parentFile.resolve(relativePath).readText()
    }
}

fun extractQuotedStrings(source: String, marker: String, count: Int): List<String> {
    val markerIndex = source.indexOf(marker)
    require(markerIndex >= 0) { "Missing marker: $marker" }
    val tail = source.substring(markerIndex)
    val matches = "\"([^\"]*)\"".toRegex().findAll(tail).map { it.groupValues[1] }.take(count).toList()
    require(matches.size == count) { "Missing quoted strings for: $marker" }
    return matches
}

fun extractRustMatchArmRawString(source: String, armName: String): String {
    val pattern = Regex(
        """"${Regex.escape(armName)}"\s*=>\s*\{\s*r(#+)"(.*?)"\1\s*\}""",
        setOf(RegexOption.DOT_MATCHES_ALL),
    )
    val match = requireNotNull(pattern.find(source)) { "Missing raw string match arm: $armName" }
    return match.groupValues[2]
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
val generatePresetOverlayAssets by tasks.registering {
    val repoRoot = rootProject.projectDir.parentFile
    val fitSource = repoRoot.resolve("src/overlay/result/markdown_view/streaming/fit_impl.rs")
    val cssSource = repoRoot.resolve("src/overlay/result/markdown_view/css.rs")
    val buttonCanvasCssSource = repoRoot.resolve("src/overlay/result/button_canvas/css.rs")
    val buttonCanvasJsSource = repoRoot.resolve("src/overlay/result/button_canvas/js.rs")
    val buttonCanvasThemeSource = repoRoot.resolve("src/overlay/result/button_canvas/theme.rs")
    val gridJsSource = repoRoot.resolve("src/overlay/html_components/grid_js.rs")
    val recordingUiSource = repoRoot.resolve("src/overlay/recording/ui.rs")
    val iconsSource = repoRoot.resolve("src/overlay/html_components/icons.rs")
    inputs.file(fitSource)
    inputs.file(cssSource)
    inputs.file(buttonCanvasCssSource)
    inputs.file(buttonCanvasJsSource)
    inputs.file(buttonCanvasThemeSource)
    inputs.file(gridJsSource)
    inputs.file(recordingUiSource)
    inputs.file(iconsSource)
    outputs.dir(generatedPresetOverlayAssets)

    doLast {
        val outputDir = generatedPresetOverlayAssets.get().asFile.resolve("preset_overlay")
        outputDir.mkdirs()

        val fitScript = extractRustConcatIncludeStrings(
            fitSource,
            "const FIT_FONT_SCRIPT: &str = concat!(",
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
        val markdownThemeSource = cssSource.readText()
        outputDir.resolve("windows_markdown_theme_dark.css").writeText(
            extractWindowsRawString(markdownThemeSource, "if is_dark {"),
        )
        outputDir.resolve("windows_markdown_theme_light.css").writeText(
            extractWindowsRawString(markdownThemeSource, "} else {"),
        )
        val (gridCssUrl, gridJsUrl) = extractQuotedStrings(
            gridJsSource.readText(),
            "pub fn get_lib_urls() -> (&'static str, &'static str) {",
            2,
        )
        outputDir.resolve("windows_gridjs_urls.json").writeText(
            """
            {
              "cssUrl": ${groovy.json.JsonOutput.toJson(gridCssUrl)},
              "jsUrl": ${groovy.json.JsonOutput.toJson(gridJsUrl)}
            }
            """.trimIndent(),
        )
        outputDir.resolve("windows_gridjs.css").writeText(
            extractWindowsRawString(
                gridJsSource.readText(),
                "pub fn get_css() -> &'static str {",
            ),
        )
        outputDir.resolve("windows_gridjs_init.js").writeText(
            extractWindowsRawString(
                gridJsSource.readText(),
                "pub fn get_init_script() -> &'static str {",
            ),
        )
        // Font dedup: skip copying — preset_overlay loads from ../GoogleSansFlex.ttf

        val staticAssetsDir = projectDir.resolve("src/main/assets/preset_overlay_static")
        if (staticAssetsDir.isDirectory) {
            staticAssetsDir.listFiles()?.forEach { file ->
                file.copyTo(outputDir.resolve(file.name), overwrite = true)
            }
        }

        outputDir.resolve("windows_button_canvas.css").writeText(
            extractWindowsRawString(
                buttonCanvasCssSource.readText(),
                "pub fn get_base_css() -> &'static str {",
            ),
        )
        outputDir.resolve("windows_button_canvas.js").writeText(
            extractWindowsRawString(
                buttonCanvasJsSource.readText(),
                "pub fn get_javascript() -> &'static str {",
            ),
        )
        val themeSource = buttonCanvasThemeSource.readText()
        outputDir.resolve("windows_button_canvas_theme_dark.css").writeText(
            extractWindowsRawString(themeSource, "if is_dark {"),
        )
        outputDir.resolve("windows_button_canvas_theme_light.css").writeText(
            extractWindowsRawString(themeSource, "} else {"),
        )

        val recordingTemplate = extractWindowsRawString(
            recordingUiSource.readText(),
            "format!(",
        )
            .replace("{{", "{")
            .replace("}}", "}")
            .replace("{font_css}", "{{FONT_CSS}}")
            .replace("{width}", "{{WINDOW_WIDTH}}")
            .replace("{height}", "{{WINDOW_HEIGHT}}")
            .replace("{tx_rec}", "{{TEXT_RECORDING}}")
            .replace("{tx_proc}", "{{TEXT_PROCESSING}}")
            .replace("{tx_wait}", "{{TEXT_WARMUP}}")
            .replace("{tx_init}", "{{TEXT_INITIALIZING}}")
            .replace("{tx_sub}", "{{TEXT_SUBTEXT}}")
            .replace("{tx_paused}", "{{TEXT_PAUSED}}")
            .replace("{icon_pause}", "{{ICON_PAUSE}}")
            .replace("{icon_play}", "{{ICON_PLAY}}")
            .replace("{icon_close}", "{{ICON_CLOSE}}")
            .replace("{container_bg}", "{{COLOR_CONTAINER_BG}}")
            .replace("{container_border}", "{{COLOR_CONTAINER_BORDER}}")
            .replace("{text_color}", "{{COLOR_TEXT}}")
            .replace("{subtext_color}", "{{COLOR_SUBTEXT}}")
            .replace("{btn_bg}", "{{COLOR_BUTTON_BG}}")
            .replace("{btn_hover_bg}", "{{COLOR_BUTTON_HOVER_BG}}")
            .replace("{btn_color}", "{{COLOR_BUTTON}}")
            .replace("{text_shadow}", "{{COLOR_TEXT_SHADOW}}")
            .replace("{is_dark}", "{{IS_DARK}}")
            .replace("<div class=\"container\">", "<div class=\"container\" id=\"container\">")
            .replaceFirst(
                "<script>",
                "<script>\n        {{BRIDGE_PRELUDE}}\n",
            )
            .replace(
                "\n    </script>\n</body>",
                "\n        {{MOBILE_SHIM}}\n    </script>\n</body>",
            )
        val iconsSourceText = iconsSource.readText()
        outputDir.resolve("windows_recording_template.html").writeText(
            recordingTemplate
                .replace("{{ICON_PAUSE}}", extractRustMatchArmRawString(iconsSourceText, "pause"))
                .replace("{{ICON_PLAY}}", extractRustMatchArmRawString(iconsSourceText, "play_arrow"))
                .replace("{{ICON_CLOSE}}", extractRustMatchArmRawString(iconsSourceText, "close")),
        )
    }
}

val generatePresetModelCatalog by tasks.registering {
    val repoRoot = rootProject.projectDir.parentFile
    val manifestSource = repoRoot.resolve("catalog/model_catalog.json")
    val generator = repoRoot.resolve("scripts/generate_android_preset_model_catalog.py")
    inputs.file(manifestSource)
    inputs.file(generator)
    outputs.dir(generatedPresetModelCatalogSources)

    doLast {
        val outputFile = generatedPresetModelCatalogSources.get()
            .asFile
            .resolve("dev/screengoated/toolbox/mobile/preset/GeneratedPresetModelCatalogData.kt")

        providers.exec {
            commandLine(
                "py",
                "-3",
                generator.absolutePath,
                "--manifest-source",
                manifestSource.absolutePath,
                "--preset-output",
                outputFile.absolutePath,
            )
        }.result.get().assertNormalExitValue()
    }
}

android {
    namespace = "dev.screengoated.toolbox.mobile"
    dynamicFeatures += setOf(
        ":feature_asr_ort",
        ":feature_asr_moonshine",
        ":feature_asr_sherpa",
        ":feature_native_cpp",
    )
    compileSdk = 36

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
        jniLibs {
            useLegacyPackaging = true
            // Only the multi-MB zip payloads are fetched at runtime. The tiny python/ffmpeg
            // wrapper binaries must stay in the APK: from Android 10 exec() is only allowed
            // out of the real nativeLibraryDir, never app-writable storage. (Sideload only —
            // the `play` flavor has no youtubedl dependency, so it has none of these.)
            excludes += "**/libpython.zip.so"
            excludes += "**/libffmpeg.zip.so"
            // All native ASR libs downloaded on demand via NativeLibManager.
            excludes += "**/libonnxruntime.so"
            excludes += "**/libonnxruntime4j_jni.so"
            excludes += "**/libc++_shared.so"
            excludes += "**/libmoonshine.so"
            excludes += "**/libmoonshine-jni.so"
            excludes += "**/libsherpa-onnx-jni.so"
        }
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
            // okhttp 5's logging-interceptor and jspecify both ship this stub.
            excludes += "/META-INF/versions/9/OSGI-INF/MANIFEST.MF"
        }
    }

    sourceSets.named("main") {
        assets.srcDir(generatedPresetOverlayAssets)
        java.srcDir(generatedPresetModelCatalogSources)
    }
}

tasks.matching {
    it.name != generatePresetOverlayAssets.name && it.name.contains("Assets", ignoreCase = false)
}.configureEach {
    dependsOn(generatePresetOverlayAssets)
}

tasks.matching {
    it.name != generatePresetModelCatalog.name &&
        (it.name.contains("Kotlin", ignoreCase = false) || it.name.contains("Java", ignoreCase = false))
}.configureEach {
    dependsOn(generatePresetModelCatalog)
}

tasks.matching {
    it.name.contains("lint", ignoreCase = true)
}.configureEach {
    dependsOn(generatePresetOverlayAssets)
    dependsOn(generatePresetModelCatalog)
}

dependencies {
    implementation(project(":shared"))

    implementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(platform(libs.androidx.compose.bom))

    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.compose.foundation)
    // material-icons-extended removed — replaced by Material Symbols vector drawables (res/drawable/ms_*.xml)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.graphics.shapes)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.core.splashscreen)
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
    implementation(libs.moonshine.voice)
    implementation(files("libs/sherpa-onnx-static-1.12.35.aar"))
    implementation(libs.androidx.media3.session)
    implementation(libs.androidx.media3.common)
    implementation(libs.commonmark)
    implementation(libs.commonmark.ext.gfm.tables)
    implementation(libs.commonmark.ext.gfm.strikethrough)
    implementation(libs.commonmark.ext.task.list.items)
    // Sideload only. Keeping these off the `play` flavor is what leaves the bundled yt-dlp
    // resource and the Python/FFmpeg payloads out of the Play artifact entirely.
    "fullImplementation"(libs.youtubedl.android.library)
    "fullImplementation"(libs.youtubedl.android.ffmpeg)
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

tasks.register("verifyPlayReleaseCompliance") {
    group = "verification"
    description = "Verifies that the Play AAB keeps executable payloads out of its base module."
    dependsOn("bundlePlayRelease")

    doLast {
        val bundle = layout.buildDirectory
            .file("outputs/bundle/playRelease/androidApp-play-release.aab")
            .get()
            .asFile
        require(bundle.isFile) { "Missing Play bundle: ${bundle.absolutePath}" }

        val forbiddenBaseNativeNames = listOf(
            "libc++_shared.so",
            "libonnxruntime.so",
            "libonnxruntime_real.so",
            "libmoonshine.so",
            "libmoonshine-jni.so",
            "libsherpa-onnx-jni.so",
            "libpython.so",
            "libpython.zip.so",
            "libffmpeg.so",
            "libffmpeg.zip.so",
            "libffprobe.so",
        )
        val forbiddenDexStrings = listOf(
            "api.github.com/repos/nganlinh4/screen-goated-toolbox",
            "api.github.com/repos/yt-dlp",
            "youtubedl-android/releases/download",
            "raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/mobile/androidApp/libs",
            "browser_download_url",
            "YoutubeDLUpdater",
            "updateYoutubeDL",
        )

        // Only these modules may ship in the Play bundle. The video downloader is absent by
        // construction: an allowlist fails closed when a new module appears.
        val allowedFeatureModules = setOf(
            "feature_asr_ort",
            "feature_asr_moonshine",
            "feature_asr_sherpa",
            "feature_native_cpp",
        )
        // Resources that must never reach Play, regardless of how they are delivered.
        val forbiddenBaseResources = listOf("base/res/raw/ytdlp")

        ZipFile(bundle).use { zip ->
            val entries = zip.entries().asSequence().toList()
            val baseNative = entries.filter { entry: ZipEntry -> entry.name.startsWith("base/lib/") }
            require(baseNative.none { entry: ZipEntry ->
                forbiddenBaseNativeNames.any { name -> entry.name.endsWith(name) }
            }) { "Play base contains an on-demand native payload" }

            val retainedResources = forbiddenBaseResources.filter { name ->
                zip.getEntry(name) != null
            }
            require(retainedResources.isEmpty()) {
                "Play base retains forbidden resources: $retainedResources"
            }

            val featureModules = entries
                .map { entry: ZipEntry -> entry.name.substringBefore('/') }
                .filter { name -> name.startsWith("feature") }
                .toSet()
            val unexpectedModules = featureModules - allowedFeatureModules
            require(unexpectedModules.isEmpty()) {
                "Play bundle ships unexpected feature modules: $unexpectedModules"
            }

            val featureNative = entries.filter { entry: ZipEntry ->
                entry.name.contains("/lib/") && !entry.name.startsWith("base/")
            }
            require(featureNative.isNotEmpty()) { "Play bundle has no native feature payloads" }
            require(featureNative.all { entry: ZipEntry -> entry.name.contains("/lib/arm64-v8a/") }) {
                "Play bundle contains an unsupported native ABI"
            }

            // Scan every base dex: R8 spills into classes2.dex and beyond as the app grows.
            val baseDexEntries = entries.filter { entry: ZipEntry ->
                entry.name.matches(Regex("base/dex/classes\\d*\\.dex"))
            }
            require(baseDexEntries.isNotEmpty()) { "Play bundle is missing base dex" }
            for (dexEntry in baseDexEntries) {
                val dexText = zip.getInputStream(dexEntry).use { input: InputStream ->
                    input.readBytes().toString(Charsets.ISO_8859_1)
                }
                val retained = forbiddenDexStrings.filter { signature -> dexText.contains(signature) }
                require(retained.isEmpty()) {
                    "Play base dex ${dexEntry.name} retains forbidden signatures: $retained"
                }
            }
        }
    }
}
