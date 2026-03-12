import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.kotlin.serialization)
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
        commonMain.dependencies {
            implementation(libs.kotlinx.coroutines.core)
            implementation(libs.kotlinx.serialization.json)
        }
        commonTest.dependencies {
            implementation(kotlin("test"))
            implementation(libs.kotlinx.coroutines.test)
        }
        androidUnitTest.dependencies {
            implementation(kotlin("test"))
            implementation(libs.junit4)
            implementation(libs.kotlinx.serialization.json)
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
