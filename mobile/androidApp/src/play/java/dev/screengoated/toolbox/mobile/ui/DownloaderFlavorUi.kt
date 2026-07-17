package dev.screengoated.toolbox.mobile.ui

import androidx.compose.runtime.Composable
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

/*
 * Play flavor carries no video downloader: yt-dlp only stays usable by updating itself
 * from the network (Device and Network Abuse), and Play separately prohibits apps that
 * facilitate downloading copyrighted content. Stubbing these here keeps the youtubedl
 * dependency, its bundled yt-dlp resource, and the Python/FFmpeg payloads out of the
 * Play artifact entirely rather than merely hiding the entry point.
 *
 * The "Download Video" card stays visible and reports the feature as unsupported.
 */

/** No-op: the Play flavor ships no downloader tools. */
@Composable
internal fun DownloaderToolsCard(
    @Suppress("UNUSED_PARAMETER") locale: MobileLocaleText,
    @Suppress("UNUSED_PARAMETER") onHelp: (Pair<String, String>) -> Unit,
) = Unit

/** No-op: unreachable on Play — the card reports the feature as unsupported instead. */
@Composable
internal fun DownloaderScreenWrapper(
    @Suppress("UNUSED_PARAMETER") locale: MobileLocaleText,
    @Suppress("UNUSED_PARAMETER") onBack: () -> Unit,
) = Unit
