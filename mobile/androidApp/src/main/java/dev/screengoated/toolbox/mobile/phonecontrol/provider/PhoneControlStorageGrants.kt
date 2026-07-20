package dev.screengoated.toolbox.mobile.phonecontrol.provider

import android.content.Context
import android.content.Intent
import android.net.Uri
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal object PhoneControlStorageGrants {
    fun persist(context: Context, uri: Uri, returnedFlags: Int): Boolean {
        val persistable = returnedFlags and (
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        )
        return runCatching {
            context.contentResolver.takePersistableUriPermission(uri, persistable)
            true
        }.getOrDefault(false)
    }

    fun grants(context: Context): AndroidProviderResult = AndroidProviderResult.Success(
        buildJsonObject {
            put(
                "persisted_grants",
                buildJsonArray {
                    context.contentResolver.persistedUriPermissions.forEach { grant ->
                        add(
                            buildJsonObject {
                                put("uri", grant.uri.toString())
                                put("read", grant.isReadPermission)
                                put("write", grant.isWritePermission)
                                put("persisted_at_ms", grant.persistedTime)
                            },
                        )
                    }
                },
            )
        },
    )

    fun hasReadableGrant(context: Context): Boolean =
        context.contentResolver.persistedUriPermissions.any { it.isReadPermission }
}
