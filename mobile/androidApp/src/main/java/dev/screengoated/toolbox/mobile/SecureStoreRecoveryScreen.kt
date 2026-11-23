package dev.screengoated.toolbox.mobile

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.ui.theme.SgtMobileTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
internal fun SecureStoreRecoveryScreen(activity: MainActivity) {
    var retrying by remember { mutableStateOf(false) }
    var failedRetry by remember { mutableStateOf(false) }
    val app = activity.application as SgtMobileApplication
    val container by app.appContainerState.collectAsStateWithLifecycle()
    LaunchedEffect(container) {
        if (container != null) activity.recreate()
    }
    SgtMobileTheme(themeMode = MobileThemeMode.SYSTEM) {
        Column(
            modifier = Modifier.fillMaxSize().padding(24.dp),
            verticalArrangement = Arrangement.Center,
        ) {
            Text(stringResource(R.string.secure_store_unavailable_title), style = MaterialTheme.typography.headlineSmall)
            Text(stringResource(R.string.secure_store_unavailable_body), modifier = Modifier.padding(vertical = 16.dp))
            if (failedRetry) Text(stringResource(R.string.secure_store_retry_failed))
            Button(enabled = !retrying, onClick = {
                retrying = true
                failedRetry = false
                activity.lifecycleScope.launch {
                    val available = withContext(Dispatchers.IO) {
                        app.retrySecureSettings()
                    }
                    if (!available) {
                        retrying = false
                        failedRetry = true
                    }
                }
            }) {
                Text(stringResource(R.string.secure_store_retry))
            }
        }
    }
}
