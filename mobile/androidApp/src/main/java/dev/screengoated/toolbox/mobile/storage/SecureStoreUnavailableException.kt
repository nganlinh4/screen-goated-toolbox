package dev.screengoated.toolbox.mobile.storage

/** The encrypted settings were not readable or writable; existing data is retained. */
class SecureStoreUnavailableException(cause: Exception) : IllegalStateException(
    "Encrypted settings are unavailable",
    cause,
)

internal inline fun <T> withSecureStoreAccess(block: () -> T): T = try {
    block()
} catch (error: SecureStoreUnavailableException) {
    throw error
} catch (error: Exception) {
    throw SecureStoreUnavailableException(error)
}
