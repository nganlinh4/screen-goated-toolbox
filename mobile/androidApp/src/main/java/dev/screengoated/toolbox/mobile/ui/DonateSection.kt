package dev.screengoated.toolbox.mobile.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

private const val DONATE_ACCOUNT = "8850273958"
private const val DONATE_BANK_LINE = "BIDV · NGUYEN BAO LINH · STK: 8850273958"
private const val VIETQR_URL =
    "https://img.vietqr.io/image/970418-8850273958-compact2.png" +
        "?accountName=NGUYEN%20BAO%20LINH&addInfo=Ung%20ho%20SGT"

// Collapsible donation card (collapsed by default), styled to match the other settings cards:
// a 40dp gradient-icon badge + title header (see ExpressiveSettingsHeader) with an expand
// chevron pinned right. The Vietnamese bank transfer (VietQR) is shown in every language;
// EN/KO add a note that it's for Vietnamese donors only. The QR is referenced by its VietQR
// image URL (opened externally), never bundled as an asset.
@Composable
internal fun DonateSection(
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val accent = MaterialTheme.colorScheme.primary
    var expanded by rememberSaveable { mutableStateOf(false) }

    ExpressiveSettingsCard(accent = accent, modifier = modifier.fillMaxWidth()) {
        Column(verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap)) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { expanded = !expanded },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
            ) {
                Box(
                    modifier = Modifier
                        .size(40.dp)
                        .background(accent.copy(alpha = 0.18f), MaterialTheme.shapes.medium),
                    contentAlignment = Alignment.Center,
                ) {
                    GradientMaskedIcon(
                        iconRes = R.drawable.ms_potted_plant,
                        brush = Brush.linearGradient(
                            listOf(accent, MaterialTheme.colorScheme.primary),
                        ),
                        modifier = Modifier.size(20.dp),
                    )
                }
                Text(
                    text = locale.donateHeader,
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.weight(1f),
                )
                Icon(
                    painter = painterResource(
                        if (expanded) R.drawable.ms_expand_less else R.drawable.ms_expand_more,
                    ),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.size(24.dp),
                )
            }

            if (expanded) {
                Text(
                    text = locale.donateBody,
                    style = MaterialTheme.typography.bodyMedium,
                )
                Text(
                    text = DONATE_BANK_LINE,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = accent,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap)) {
                    ExpressiveSettingsButton(
                        text = "Sao chép STK",
                        onClick = {
                            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE)
                                as ClipboardManager
                            clipboard.setPrimaryClip(ClipData.newPlainText("STK", DONATE_ACCOUNT))
                        },
                        accent = accent,
                    )
                    ExpressiveSettingsButton(
                        text = "Mở mã VietQR",
                        onClick = {
                            runCatching {
                                context.startActivity(
                                    Intent(Intent.ACTION_VIEW, VIETQR_URL.toUri())
                                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                                )
                            }
                        },
                        accent = accent,
                    )
                }
                // EN/KO get an extra line clarifying the bank transfer is for Vietnamese donors.
                if (!locale.donateVietnamese) {
                    Text(
                        text = locale.donateNote,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}
