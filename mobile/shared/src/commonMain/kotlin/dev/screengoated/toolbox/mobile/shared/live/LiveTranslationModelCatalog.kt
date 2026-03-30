package dev.screengoated.toolbox.mobile.shared.live

object LiveTranslationModelCatalog {
    const val PROVIDER_CEREBRAS = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_CEREBRAS
    const val PROVIDER_GEMMA = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_GEMMA
    const val PROVIDER_GTX = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_GTX

    const val CEREBRAS_API_MODEL = GeneratedLiveModelCatalog.CEREBRAS_API_MODEL
    const val GEMMA_API_MODEL = GeneratedLiveModelCatalog.GEMMA_API_MODEL
    const val GTX_API_MODEL = GeneratedLiveModelCatalog.GTX_API_MODEL

    fun providerDescriptor(id: String): ProviderDescriptor {
        return GeneratedLiveModelCatalog.translationProviderDescriptor(
            when (id) {
                "taalas-rt" -> PROVIDER_CEREBRAS
                PROVIDER_CEREBRAS, PROVIDER_GEMMA, PROVIDER_GTX -> id
                else -> GeneratedLiveModelCatalog.DEFAULT_TRANSLATION_PROVIDER_ID
            },
        )
    }
}
