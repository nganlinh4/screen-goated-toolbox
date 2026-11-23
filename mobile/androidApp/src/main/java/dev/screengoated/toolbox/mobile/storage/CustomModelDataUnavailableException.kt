package dev.screengoated.toolbox.mobile.storage

/** Custom-model bytes are readable but cannot be interpreted by this version. */
internal class CustomModelDataUnavailableException(cause: Exception) : IllegalStateException(
    "Custom-model data is unavailable",
    cause,
)
