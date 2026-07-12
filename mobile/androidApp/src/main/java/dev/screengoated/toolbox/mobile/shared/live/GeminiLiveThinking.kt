package dev.screengoated.toolbox.mobile.shared.live

import org.json.JSONObject

fun geminiLiveThinkingJson(apiModel: String): JSONObject? = when (
    val config = GeneratedLiveModelCatalog.thinkingConfig(apiModel)
) {
    is GeneratedLiveThinkingConfig.Budget -> JSONObject().put("thinkingBudget", config.value)
    is GeneratedLiveThinkingConfig.Level -> JSONObject().put("thinkingLevel", config.value)
    null -> null
}
