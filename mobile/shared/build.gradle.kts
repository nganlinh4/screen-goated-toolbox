import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.kotlin.serialization)
}

val generatedLiveModelCatalogSources = layout.buildDirectory.dir("generated/liveModelCatalog")

val generateLiveModelCatalog by tasks.registering {
    val repoRoot = rootProject.projectDir.parentFile
    val manifestSource = repoRoot.resolve("catalog/model_catalog.json")
    val generator = repoRoot.resolve("scripts/generate_android_preset_model_catalog.py")
    inputs.file(manifestSource)
    inputs.file(generator)
    outputs.dir(generatedLiveModelCatalogSources)

    doLast {
        val outputFile = generatedLiveModelCatalogSources.get()
            .asFile
            .resolve("dev/screengoated/toolbox/mobile/shared/live/GeneratedLiveModelCatalog.kt")

        providers.exec {
            commandLine(
                "py",
                "-3",
                generator.absolutePath,
                "--manifest-source",
                manifestSource.absolutePath,
                "--live-output",
                outputFile.absolutePath,
            )
        }.result.get().assertNormalExitValue()
    }
}

kotlin {
    androidTarget {
        compilerOptions {
            jvmTarget.set(JvmTarget.JVM_17)
        }
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
        val androidUnitTest by getting {
            dependencies {
                implementation(kotlin("test"))
                implementation(libs.junit4)
                implementation(libs.kotlinx.serialization.json)
            }
        }
    }
}

android {
    namespace = "dev.screengoated.toolbox.mobile.shared"
    compileSdk = 36

    defaultConfig {
        minSdk = 29
    }
}

tasks.matching {
    it.name.contains("Kotlin", ignoreCase = false)
}.configureEach {
    dependsOn(generateLiveModelCatalog)
}
