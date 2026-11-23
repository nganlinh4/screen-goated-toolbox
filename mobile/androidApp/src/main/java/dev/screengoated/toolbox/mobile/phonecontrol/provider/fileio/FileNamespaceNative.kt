package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio

import androidx.annotation.Keep

@Keep
internal object FileNamespaceNative {
    init { System.loadLibrary("sgt_file_namespace") }
    external fun openDirectory(path: ByteArray): Int
    external fun openEntry(directory: Int, name: ByteArray, kind: Int): Int
    external fun syncDirectory(directory: Int): Int
    external fun lockFile(descriptor: Int, acquire: Boolean): Int
    external fun makeDirectory(directory: Int, name: ByteArray): Int
    external fun publish(directory: Int, source: ByteArray, target: ByteArray, exchange: Boolean): Int
    external fun moveNoReplace(sourceDirectory: Int, source: ByteArray, targetDirectory: Int, target: ByteArray): Int
    external fun unlinkStagedFile(directory: Int, name: ByteArray, expectedFd: Int): Int
}
