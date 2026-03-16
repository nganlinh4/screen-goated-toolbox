package dev.screengoated.toolbox.mobile.shared.live

object LiveTranslationModelCatalog {
    const val PROVIDER_CEREBRAS = "cerebras-oss"
    const val PROVIDER_GEMMA = "google-gemma"
    const val PROVIDER_GTX = "google-gtx"

    const val CEREBRAS_API_MODEL = "qwen-3-235b-a22b-instruct-2507"
    const val GEMMA_API_MODEL = "gemma-3-27b-it"
    const val GTX_API_MODEL = "google-translate-gtx"

    fun providerDescriptor(id: String): ProviderDescriptor {
        return when (id) {
            PROVIDER_CEREBRAS -> ProviderDescriptor(
                id = PROVIDER_CEREBRAS,
                model = CEREBRAS_API_MODEL,
            )

            PROVIDER_GTX -> ProviderDescriptor(
                id = PROVIDER_GTX,
                model = GTX_API_MODEL,
            )

            else -> ProviderDescriptor(
                id = PROVIDER_GEMMA,
                model = GEMMA_API_MODEL,
            )
        }
    }
}
