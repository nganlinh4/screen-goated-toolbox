package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.net.Uri
import android.provider.MediaStore
import android.provider.OpenableColumns
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.UUID
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CreationDownloadsStoreTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()

    @Test
    fun verifiedSvgPublishesAsAVisibleDownloadsDocument() {
        val token = UUID.randomUUID().toString()
        val source = File(context.cacheDir, "creation-downloads-$token.svg")
        source.writeText("<svg xmlns=\"http://www.w3.org/2000/svg\"/>")
        val size = source.length()
        val digest = creationFileSha256(source)
        val outputs = CreationOutputStore(context) {}
        val intent = outputs.plan(
            token,
            "creation-downloads-$token.svg",
            "image/svg+xml",
            MediaStore.Downloads.EXTERNAL_CONTENT_URI.toString(),
            emptyList(),
        )
        val pending = outputs.reserve(intent)
        var published: String? = null
        try {
            outputs.populate(intent, pending.handle, pending.identity, source, size, digest)
            published = outputs.commit(intent, pending.handle, pending.identity, size, digest)
            val uri = Uri.parse(published)
            assertEquals("media", uri.authority)
            assertEquals("downloads", uri.pathSegments.getOrNull(1))
            assertEquals(intent.finalName, displayName(uri))
            assertTrue(outputs.publishedArtifactMatches(published, pending.identity, size, digest))
        } finally {
            published?.let(outputs::delete)
                ?: outputs.abort(intent, pending.handle, pending.identity)
            source.delete()
        }
    }

    private fun displayName(uri: Uri): String? = context.contentResolver.query(
        uri,
        arrayOf(OpenableColumns.DISPLAY_NAME),
        null,
        null,
        null,
    )?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
}
