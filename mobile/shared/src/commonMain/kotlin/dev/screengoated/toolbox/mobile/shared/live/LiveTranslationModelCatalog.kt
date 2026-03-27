package dev.screengoated.toolbox.mobile.shared.live

object LiveTranslationModelCatalog {
    const val PROVIDER_TAALAS = "taalas-rt"
    const val PROVIDER_GEMMA = "google-gemma"
    const val PROVIDER_GTX = "google-gtx"

    const val TAALAS_API_MODEL = "llama3.1-8B"
    const val GEMMA_API_MODEL = "gemma-3-27b-it"
    const val GTX_API_MODEL = "google-translate-gtx"

    fun providerDescriptor(id: String): ProviderDescriptor {
        return when (id) {
            PROVIDER_TAALAS -> ProviderDescriptor(
                id = PROVIDER_TAALAS,
                model = TAALAS_API_MODEL,
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
