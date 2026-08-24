import org.gradle.api.tasks.Exec
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.kotlin.multiplatform.library)
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.kotlin.serialization)
}

val generatedLiveModelCatalogSources = layout.buildDirectory.dir("generated/liveModelCatalog")
val generatedPresetDefaultModelsSources = layout.buildDirectory.dir("generated/presetDefaultModels")

val generateLiveModelCatalog by tasks.registering(Exec::class) {
    val repoRoot = rootProject.projectDir.parentFile
    val manifestSource = repoRoot.resolve("catalog/model_catalog.json")
    val generator = repoRoot.resolve("scripts/generate_android_preset_model_catalog.py")
    inputs.file(manifestSource)
    inputs.file(generator)
    val outputFile = generatedLiveModelCatalogSources.get()
        .asFile
        .resolve("dev/screengoated/toolbox/mobile/shared/live/GeneratedLiveModelCatalog.kt")
    outputs.file(outputFile)
    commandLine(
        "py",
        "-3",
        generator.absolutePath,
        "--manifest-source",
        manifestSource.absolutePath,
        "--live-output",
        outputFile.absolutePath,
    )
}

val generatePresetDefaultModels by tasks.registering(Exec::class) {
    val repoRoot = rootProject.projectDir.parentFile
    val manifestSource = repoRoot.resolve("catalog/model_catalog.json")
    val generator = repoRoot.resolve("scripts/generate_android_preset_model_catalog.py")
    inputs.file(manifestSource)
    inputs.file(generator)
    val outputFile = generatedPresetDefaultModelsSources.get()
        .asFile
        .resolve("dev/screengoated/toolbox/mobile/shared/preset/GeneratedPresetDefaultModels.kt")
    outputs.file(outputFile)
    commandLine(
        "py",
        "-3",
        generator.absolutePath,
        "--manifest-source",
        manifestSource.absolutePath,
        "--preset-defaults-output",
        outputFile.absolutePath,
    )
}

kotlin {
    android {
        namespace = "dev.screengoated.toolbox.mobile.shared"
        compileSdk = 36
        minSdk = 29
        compilerOptions {
            jvmTarget.set(JvmTarget.JVM_17)
        }
        withHostTest {}
    }

    listOf(
        iosX64(),
        iosArm64(),
        iosSimulatorArm64(),
    ).forEach { iosTarget ->
        iosTarget.binaries.framework {
            baseName = "SgtMobileShared"
            isStatic = true
        }
    }

    jvmToolchain(17)

    sourceSets {
        val commonMain by getting {
            kotlin.srcDir(generatedLiveModelCatalogSources)
            kotlin.srcDir(generatedPresetDefaultModelsSources)
            dependencies {
                implementation(libs.kotlinx.coroutines.core)
                implementation(libs.kotlinx.serialization.json)
            }
        }
        val commonTest by getting {
            dependencies {
                implementation(kotlin("test"))
                implementation(libs.kotlinx.coroutines.test)
            }
        }
        val androidHostTest by getting {
            dependencies {
                implementation(kotlin("test"))
                implementation(libs.junit4)
                implementation(libs.kotlinx.serialization.json)
            }
        }
    }
}

tasks.matching {
    it.name.contains("Kotlin", ignoreCase = false)
}.configureEach {
    dependsOn(generateLiveModelCatalog)
    dependsOn(generatePresetDefaultModels)
}
