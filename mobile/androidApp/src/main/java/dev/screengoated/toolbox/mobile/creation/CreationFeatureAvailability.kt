package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.BuildConfig

internal fun creationToolReleased(tool: CreationTool): Boolean =
    tool != CreationTool.IMAGE_CREATOR || BuildConfig.IMAGE_CREATOR_RELEASE_ENABLED
