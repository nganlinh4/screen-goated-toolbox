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
    val end = source.indexOf("\"#", contentStart)
    require(end >= 0) { "Missing raw string end for: $marker" }
    return source.substring(contentStart, end)
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
    val modelSource = repoRoot.resolve("src/model_config.rs")
    val configSource = repoRoot.resolve("src/config/config.rs")
    val prioritySource = repoRoot.resolve("src/config/types/model_priority.rs")
    val generator = repoRoot.resolve("scripts/generate_android_preset_model_catalog.py")
    inputs.file(modelSource)
    inputs.file(configSource)
    inputs.file(prioritySource)
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
                "--model-source",
                modelSource.absolutePath,
                "--config-source",
                configSource.absolutePath,
                "--priority-source",
                prioritySource.absolutePath,
                "--output",
                outputFile.absolutePath,
            )
        }.result.get().assertNormalExitValue()
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

        ndk {
            abiFilters += "arm64-v8a"
        }
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
            isMinifyEnabled = true
            isShrinkResources = true
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
        jniLibs {
            useLegacyPackaging = true
            excludes += "**/libonnxruntime.so"
            excludes += "**/libonnxruntime4j_jni.so"
            excludes += "**/libpython.zip.so"
            excludes += "**/libffmpeg.zip.so"
        }
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
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
    it.name.contains("Lint", ignoreCase = false)
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
    implementation(libs.androidx.media3.session)
    implementation(libs.androidx.media3.common)
    implementation("org.commonmark:commonmark:0.24.0")
    implementation("org.commonmark:commonmark-ext-gfm-tables:0.24.0")
    implementation("org.commonmark:commonmark-ext-gfm-strikethrough:0.24.0")
    implementation("org.commonmark:commonmark-ext-task-list-items:0.24.0")
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
