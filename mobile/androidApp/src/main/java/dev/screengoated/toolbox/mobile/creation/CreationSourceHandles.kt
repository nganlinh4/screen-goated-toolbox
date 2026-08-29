package dev.screengoated.toolbox.mobile.creation

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri

internal fun persistCreationSourceHandle(resolver: ContentResolver, uri: Uri): Boolean {
    runCatching {
        resolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    return resolver.persistedUriPermissions.any {
        it.uri == uri && it.isReadPermission
    }
}
