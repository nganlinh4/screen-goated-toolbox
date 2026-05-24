package dev.screengoated.toolbox.mobile.shared.live

object LiveTranslationModelCatalog {
    const val PROVIDER_LLM = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_LLM
    const val PROVIDER_GTX = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_GTX

    const val GTX_API_MODEL = GeneratedLiveModelCatalog.GTX_API_MODEL

    fun providerDescriptor(id: String): ProviderDescriptor {
        return GeneratedLiveModelCatalog.translationProviderDescriptor(
            when (id) {
                PROVIDER_LLM, PROVIDER_GTX -> id
                else -> GeneratedLiveModelCatalog.DEFAULT_TRANSLATION_PROVIDER_ID
            },
        )
    }
}
