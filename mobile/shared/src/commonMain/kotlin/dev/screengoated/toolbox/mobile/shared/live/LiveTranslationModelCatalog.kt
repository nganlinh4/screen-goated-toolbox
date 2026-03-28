package dev.screengoated.toolbox.mobile.shared.live

object LiveTranslationModelCatalog {
    const val PROVIDER_TAALAS = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_TAALAS
    const val PROVIDER_GEMMA = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_GEMMA
    const val PROVIDER_GTX = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_GTX

    const val TAALAS_API_MODEL = GeneratedLiveModelCatalog.TAALAS_API_MODEL
    const val GEMMA_API_MODEL = GeneratedLiveModelCatalog.GEMMA_API_MODEL
    const val GTX_API_MODEL = GeneratedLiveModelCatalog.GTX_API_MODEL

    fun providerDescriptor(id: String): ProviderDescriptor {
        return GeneratedLiveModelCatalog.translationProviderDescriptor(id)
    }
}
