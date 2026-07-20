package dev.screengoated.toolbox.mobile.service.nativelibs

/**
 * Flavor-neutral native dependency order.
 *
 * The packaged ONNX proxy exports only the API-table entry point. Load the real
 * runtime first so native consumers whose ABI imports other ONNX symbols bind to
 * the real library's canonical SONAME. The proxy remains an installed payload
 * member for compatibility, but is not itself a runtime prerequisite.
 */
internal object NativeLibraryLoadContract {
    private val dependencyOrder = listOf(
        "libc++_shared.so",
        "libonnxruntime_real.so",
        "libmoonshine.so",
        "libmoonshine-jni.so",
        "libsherpa-onnx-jni.so",
    )
    private val installedOnly = setOf("libonnxruntime.so")

    fun orderedDependencies(needed: Iterable<String>): List<String> {
        val requested = needed.distinct()
        val known = dependencyOrder.filter(requested::contains)
        val future = requested.filter { it !in dependencyOrder && it !in installedOnly }
        return known + future
    }
}
