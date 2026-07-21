package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.SgtMobileTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class CreationMiniAppActivity : ComponentActivity() {
    private lateinit var tool: CreationTool
    private lateinit var viewModel: CreationNativeViewModel

    private val imagePicker = registerForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments(),
    ) { uris ->
        if (uris.isEmpty()) return@registerForActivityResult
        lifecycleScope.launch {
            val paths = withContext(Dispatchers.IO) {
                CreationJobManager.get(this@CreationMiniAppActivity).files.importImages(uris)
            }
            viewModel.addImages(paths)
        }
    }

    private val outputPicker = registerForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        uri?.let(viewModel::rememberOutputDirectory)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        tool = CreationTool.fromWireName(intent.getStringExtra(EXTRA_TOOL)) ?: run {
            finish()
            return
        }
        enableEdgeToEdge()
        viewModel = ViewModelProvider(
            this,
            CreationNativeViewModel.Factory(application, tool),
        )[CreationNativeViewModel::class.java]
        val preferences = (application as SgtMobileApplication).appContainer.repository
            .currentUiPreferences()
        val locale = MobileLocaleText.forLanguage(preferences.uiLanguage)

        setContent {
            val state = viewModel.state.collectAsStateWithLifecycle().value
            SgtMobileTheme(themeMode = preferences.themeMode) {
                CreationNativeScreen(
                    tool = tool,
                    state = state,
                    locale = locale,
                    viewModel = viewModel,
                    onBack = ::finish,
                    onPickImages = {
                        imagePicker.launch(arrayOf("image/png", "image/jpeg", "image/webp"))
                    },
                    onPickOutputDirectory = { outputPicker.launch(null) },
                )
            }
        }
        onBackPressedDispatcher.addCallback(
            this,
            object : androidx.activity.OnBackPressedCallback(true) {
                override fun handleOnBackPressed() = finish()
            },
        )
    }

    companion object {
        private const val EXTRA_TOOL = "creation_tool"

        internal fun intent(context: Context, tool: CreationTool): Intent = Intent(
            context,
            CreationMiniAppActivity::class.java,
        ).putExtra(EXTRA_TOOL, tool.wireName)
    }
}
