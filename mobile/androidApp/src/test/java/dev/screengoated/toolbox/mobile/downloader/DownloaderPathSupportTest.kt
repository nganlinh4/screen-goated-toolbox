package dev.screengoated.toolbox.mobile.downloader

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DownloaderPathSupportTest {
    @Test
    fun primaryTreePathMapsToSharedStoragePath() {
        assertEquals(
            "/storage/emulated/0/Download/SGT",
            downloadTreePathToFilesystemPath("/tree/primary:Download/SGT"),
        )
    }

    @Test
    fun percentEncodedPrimaryTreePathMapsToSharedStoragePath() {
        assertEquals(
            "/storage/emulated/0/Download/SGT Clips",
            downloadTreePathToFilesystemPath("/tree/primary%3ADownload%2FSGT%20Clips"),
        )
    }

    @Test
    fun plusInEncodedPathStaysLiteral() {
        assertEquals(
            "/storage/emulated/0/Download/SGT+Clips",
            downloadTreePathToFilesystemPath("/tree/primary%3ADownload%2FSGT+Clips"),
        )
    }

    @Test
    fun primaryRootMapsToSharedStorageRoot() {
        assertEquals(
            "/storage/emulated/0",
            downloadTreePathToFilesystemPath("/tree/primary:"),
        )
    }

    @Test
    fun nonPrimaryTreePathIsRejectedBecauseYtdlpNeedsFilesystemPath() {
        assertNull(downloadTreePathToFilesystemPath("/tree/1234-5678:Download/SGT"))
    }

    @Test
    fun malformedTreePathIsRejected() {
        assertNull(downloadTreePathToFilesystemPath("/document/primary:Download/SGT"))
        assertNull(downloadTreePathToFilesystemPath("/tree/primary"))
        assertNull(downloadTreePathToFilesystemPath(null))
    }
}
