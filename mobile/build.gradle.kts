plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.dynamic.feature) apply false
    alias(libs.plugins.android.kotlin.multiplatform.library) apply false
    alias(libs.plugins.compose.compiler) apply false
    alias(libs.plugins.kotlin.multiplatform) apply false
    alias(libs.plugins.kotlin.serialization) apply false
}

val nativeRuntimeContractFile =
    rootProject.projectDir.parentFile.resolve("parity-fixtures/phone-control/native-runtime-contract.json")
val nativeRuntimeArchiveDir = rootProject.projectDir.resolve("androidApp/libs")
val sherpaRuntimeSpecDir = rootProject.projectDir.resolve("native/sherpa-runtime")
val sherpaRuntimeBuildContractFile = sherpaRuntimeSpecDir.resolve("build-contract.json")
val nativeRuntimeVerifier = rootProject.projectDir.resolve("scripts/verify_native_runtime_archives.py")

tasks.register<Exec>("verifyNativeRuntimeArchives") {
    group = "verification"
    description = "Verifies checked-in native runtime archives against the parity contract."
    inputs.file(nativeRuntimeContractFile)
    inputs.files(fileTree(nativeRuntimeArchiveDir) { include("*-runtime.zip") })
    inputs.dir(sherpaRuntimeSpecDir)
    inputs.file(nativeRuntimeVerifier)
    commandLine(
        "py",
        "-3",
        nativeRuntimeVerifier.absolutePath,
        "--mobile-root",
        rootProject.projectDir.absolutePath,
        "--contract",
        nativeRuntimeContractFile.absolutePath,
        "--archive-dir",
        nativeRuntimeArchiveDir.absolutePath,
        "--sherpa-spec-dir",
        sherpaRuntimeSpecDir.absolutePath,
        "--sherpa-build-contract",
        sherpaRuntimeBuildContractFile.absolutePath,
    )
}
