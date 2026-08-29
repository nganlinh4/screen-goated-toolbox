package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.BuildConfig

internal fun creationToolReleased(tool: CreationTool): Boolean = when (tool) {
    CreationTool.IMAGE_TO_3D -> true
    CreationTool.IMAGE_TO_SVG -> BuildConfig.IMAGE_TO_SVG_RELEASE_ENABLED
    CreationTool.IMAGE_CREATOR -> BuildConfig.IMAGE_CREATOR_RELEASE_ENABLED
}
