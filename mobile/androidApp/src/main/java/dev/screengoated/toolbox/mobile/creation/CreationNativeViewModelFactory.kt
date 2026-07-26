package dev.screengoated.toolbox.mobile.creation

import android.app.Application
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider

internal class CreationNativeViewModelFactory(
    private val application: Application,
    private val tool: CreationTool,
) : ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T =
        CreationNativeViewModel(application, tool) as T
}
