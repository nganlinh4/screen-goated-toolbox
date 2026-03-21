package dev.screengoated.toolbox.mobile.service.preset

internal fun appendDynamicUserRequest(
    existingPrompt: String,
    userPrompt: String,
): String {
    return if (existingPrompt.isBlank()) {
        userPrompt
    } else {
        "$existingPrompt\n\nUser request: $userPrompt"
    }
}
