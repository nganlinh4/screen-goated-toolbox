package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import java.lang.ref.WeakReference

internal fun PresetResultWindowState.withInputAdapterMedia(input: PresetInput, content: String): PresetResultWindowState =
    copy(
        inputAudioBytes = (input as? PresetInput.Audio)?.wavBytes,
        inputAudioHtml = if (input is PresetInput.Audio) WeakReference(content) else null,
        inputImageBytes = (input as? PresetInput.Image)?.pngBytes,
        inputImageHtml = if (input is PresetInput.Image) WeakReference(content) else null,
    )
