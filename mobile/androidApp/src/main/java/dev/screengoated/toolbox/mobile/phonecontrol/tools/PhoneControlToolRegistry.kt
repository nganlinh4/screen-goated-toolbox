package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import java.util.Locale

internal enum class PhoneControlHandler(
    val mutating: Boolean,
) {
    OBSERVE(false),
    ACT(true),
    DO_STEPS(true),
    CLICK_AT(true),
    ZOOM(false),
    RESET_VIEW(false),
    SEE_WHOLE_SCREEN(false),
    LOOK(false),
    CLICK_TARGET(true),
    MAP_TARGETS(false),
    CLICK_MARK(true),
    WAIT(false),
    TYPE_TEXT(true),
    SCROLL(true),
    DRAG(true),
    DRAG_TARGET(true),
    KEY_COMBINATION(true),
    OPEN_URL(true),
    LAUNCH_APP(true),
    SYSTEM_QUERY(false),
    LIST_FILES(false),
    READ_TEXT_FILE(false),
    EDIT_TEXT_FILE(true),
    EDIT_TEXT_FILE_STRUCTURE(true),
    RUN_COMMAND(true),
    FOCUS_WINDOW(true),
    LIST_WINDOWS(false),
    MINIMIZE_WINDOW(true),
    RESIZE_WINDOW(true),
    MOVE_WINDOW(true),
    READ_CLIPBOARD(false),
    ARTIFACT_INFO(false),
    EXTRACT_ARTIFACT(true),
    SAVE_ARTIFACT(true),
    PASTE_ARTIFACT(true),
    SEARCH_MEMORY(false),
    OPEN_MEMORY(false),
    BROWSER_SETUP(false),
    BROWSER_STATUS(false),
    BROWSER_RESET(false),
    BROWSER_READ_PAGE(false),
    RESEARCH_WEB(false),
    BROWSER_EXTRACT_PAGE(false),
    BROWSER_WAIT_FOR(false),
    BROWSER_EVAL(true),
    BROWSER_NAVIGATE(true),
    BROWSER_HISTORY(true),
    BROWSER_OPEN_TAB(true),
    BROWSER_UPLOAD(true),
    BROWSER_TABS(false),
    BROWSER_SWITCH_TAB(true),
    BROWSER_CLOSE_TAB(true),
    BROWSER_NETWORK(false),
    BROWSER_CONSOLE(false),
    DONE(false),
}

internal data class PhoneControlToolSpec(
    val name: String,
    val capability: String,
    /** Primary receipt providers this exact handler may select, in capability-route order. */
    val providerIds: List<String>,
    /** Non-effectful prerequisites allowed to return a typed dependency receipt. */
    val dependencyProviderIds: Set<String> = emptySet(),
    val unavailableState: CapabilityState,
    val requiredUserStep: String? = null,
    val handler: PhoneControlHandler? = null,
) {
    init {
        require(name.isNotBlank())
        require(capability.isNotBlank())
        require(providerIds.isNotEmpty())
        require(providerIds.none(String::isBlank))
        require(providerIds.distinct().size == providerIds.size)
        require(dependencyProviderIds.none(String::isBlank))
        require(dependencyProviderIds.intersect(providerIds.toSet()).isEmpty())
    }

    val requiresMutationAcknowledgement: Boolean
        get() = handler?.mutating == true
}

/**
 * Execution metadata only. Declarations and parameter schemas remain owned by
 * the generated canonical catalog; the parity test prevents name drift.
 */
internal object PhoneControlToolRegistry {
    val specs: List<PhoneControlToolSpec> = listOf(
        real("observe", "ui.semantic_observe", "accessibility", PhoneControlHandler.OBSERVE),
        real("act", "ui.pointer_action", "accessibility", PhoneControlHandler.ACT),
        real("do_steps", "ui.pointer_action", "accessibility", PhoneControlHandler.DO_STEPS),
        realWithProviders(
            "click_at",
            "ui.pointer_action",
            POINTER_EFFECT_PROVIDERS,
            PhoneControlHandler.CLICK_AT,
        ),
        real("zoom", "ui.visual_observe", "accessibility", PhoneControlHandler.ZOOM),
        real("reset_view", "ui.visual_observe", "accessibility", PhoneControlHandler.RESET_VIEW),
        real(
            "see_whole_screen",
            "ui.visual_observe",
            "accessibility",
            PhoneControlHandler.SEE_WHOLE_SCREEN,
        ),
        real("look", "ui.visual_observe", "accessibility", PhoneControlHandler.LOOK),
        realWithProviders(
            "click_target",
            "ui.pointer_action",
            listOf("current_frame_vision"),
            PhoneControlHandler.CLICK_TARGET,
            dependencyProviders = POINTER_EFFECT_PROVIDERS.toSet(),
        ),
        real(
            "map_targets",
            "blind_surface_grounding",
            "current_frame_vision",
            PhoneControlHandler.MAP_TARGETS,
        ),
        realWithProviders(
            "click_mark",
            "ui.pointer_action",
            listOf("current_frame_vision"),
            PhoneControlHandler.CLICK_MARK,
            dependencyProviders = POINTER_EFFECT_PROVIDERS.toSet(),
        ),
        real("wait", "local_completion_and_cleanup", "android_app_api", PhoneControlHandler.WAIT),
        realWithProviders(
            "type_text",
            "ui.text_edit",
            listOf("accessibility", "accessibility_input_method"),
            PhoneControlHandler.TYPE_TEXT,
        ),
        realWithProviders(
            "scroll",
            "ui.pointer_action",
            POINTER_EFFECT_PROVIDERS,
            PhoneControlHandler.SCROLL,
        ),
        realWithProviders(
            "drag",
            "ui.pointer_action",
            POINTER_EFFECT_PROVIDERS,
            PhoneControlHandler.DRAG,
        ),
        realWithProviders(
            "drag_target",
            "ui.pointer_action",
            listOf("current_frame_vision"),
            PhoneControlHandler.DRAG_TARGET,
            dependencyProviders = POINTER_EFFECT_PROVIDERS.toSet(),
        ),
        unsupported("click_here", "ui.pointer_action", "accessibility"),
        unavailable("point_at", "ui.pointer_action", "current_frame_vision"),
        realWithProviders(
            "key_combination",
            "ui.key_action",
            listOf("accessibility", "accessibility_input_method"),
            PhoneControlHandler.KEY_COMBINATION,
        ),
        real(
            "open_url",
            "browser_authenticated_navigation",
            "android_app_api",
            PhoneControlHandler.OPEN_URL,
        ),
        realWithProviders(
            "launch_app",
            "app_and_task_control",
            listOf("android_app_api", "sgt_adb_bridge", "shizuku_shell", "root_bridge"),
            PhoneControlHandler.LAUNCH_APP,
            dependencyProviders = setOf("accessibility"),
        ),
        realWithProviders(
            "system_query",
            "system_query",
            listOf("android_app_api", "accessibility"),
            PhoneControlHandler.SYSTEM_QUERY,
        ),
        realWithProviders(
            "list_files",
            "file_resource_access",
            listOf("android_app_api", "sgt_adb_bridge", "shizuku_shell", "root_bridge"),
            PhoneControlHandler.LIST_FILES,
        ),
        real(
            "read_text_file",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.READ_TEXT_FILE,
        ),
        real(
            "edit_text_file",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.EDIT_TEXT_FILE,
        ),
        real(
            "edit_text_file_structure",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.EDIT_TEXT_FILE_STRUCTURE,
        ),
        realWithProviders(
            "run_command",
            "command_execution",
            listOf("sgt_adb_bridge", "shizuku_shell", "root_bridge"),
            PhoneControlHandler.RUN_COMMAND,
        ),
        realWithProviders(
            "focus_window",
            "app_and_task_control",
            listOf("android_app_api", "accessibility"),
            PhoneControlHandler.FOCUS_WINDOW,
        ),
        real("list_windows", "app_and_task_control", "accessibility", PhoneControlHandler.LIST_WINDOWS),
        real(
            "minimize_window",
            "app_and_task_control",
            "accessibility",
            PhoneControlHandler.MINIMIZE_WINDOW,
        ),
        real(
            "resize_window",
            "app_and_task_control",
            "privileged_system",
            PhoneControlHandler.RESIZE_WINDOW,
        ),
        real(
            "move_window",
            "app_and_task_control",
            "privileged_system",
            PhoneControlHandler.MOVE_WINDOW,
        ),
        real("read_clipboard", "system_query", "accessibility", PhoneControlHandler.READ_CLIPBOARD),
        real(
            "artifact_info",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.ARTIFACT_INFO,
        ),
        real(
            "extract_artifact",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.EXTRACT_ARTIFACT,
        ),
        real(
            "save_artifact",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.SAVE_ARTIFACT,
        ),
        realWithProviders(
            "paste_artifact",
            "ui.text_edit",
            listOf("accessibility", "accessibility_input_method"),
            PhoneControlHandler.PASTE_ARTIFACT,
            dependencyProviders = setOf("android_app_api"),
        ),
        real("done", "local_completion_and_cleanup", "android_app_api", PhoneControlHandler.DONE),
        real("search_memory", "system_query", "android_app_api", PhoneControlHandler.SEARCH_MEMORY),
        real("open_memory", "system_query", "android_app_api", PhoneControlHandler.OPEN_MEMORY),
        realWithProviders(
            "browser_setup",
            "browser_authenticated_navigation",
            listOf("custom_tabs_session"),
            PhoneControlHandler.BROWSER_SETUP,
            dependencyProviders = setOf("accessibility"),
        ),
        realWithProviders(
            "browser_status",
            "browser_semantic",
            listOf("browser_cdp", "accessibility"),
            PhoneControlHandler.BROWSER_STATUS,
            dependencyProviders = setOf("custom_tabs_session"),
        ),
        real(
            "browser_reset",
            "browser_devtools",
            "browser_cdp",
            PhoneControlHandler.BROWSER_RESET,
        ),
        realWithProviders(
            "browser_read_page",
            "browser_semantic",
            listOf("browser_cdp", "accessibility"),
            PhoneControlHandler.BROWSER_READ_PAGE,
            dependencyProviders = setOf("custom_tabs_session"),
        ),
        real(
            "research_web",
            "public_web_research",
            "direct_web_research",
            PhoneControlHandler.RESEARCH_WEB,
        ),
        realWithProviders(
            "browser_extract_page",
            "browser_semantic",
            listOf("browser_cdp", "accessibility"),
            PhoneControlHandler.BROWSER_EXTRACT_PAGE,
            dependencyProviders = setOf("custom_tabs_session"),
        ),
        real(
            "browser_wait_for",
            "browser_semantic",
            "browser_cdp",
            PhoneControlHandler.BROWSER_WAIT_FOR,
        ),
        real(
            "browser_eval",
            "browser_devtools",
            "browser_cdp",
            PhoneControlHandler.BROWSER_EVAL,
        ),
        realWithProviders(
            "browser_navigate",
            "browser_authenticated_navigation",
            listOf("custom_tabs_session", "browser_cdp"),
            PhoneControlHandler.BROWSER_NAVIGATE,
            dependencyProviders = setOf("accessibility"),
        ),
        realWithProviders(
            "browser_history",
            "browser_authenticated_navigation",
            listOf("browser_cdp", "accessibility"),
            PhoneControlHandler.BROWSER_HISTORY,
            dependencyProviders = setOf("custom_tabs_session"),
        ),
        real(
            "browser_open_tab",
            "browser_devtools",
            "browser_cdp",
            PhoneControlHandler.BROWSER_OPEN_TAB,
        ),
        real(
            "browser_upload",
            "browser_devtools",
            "browser_cdp",
            PhoneControlHandler.BROWSER_UPLOAD,
        ),
        real(
            "browser_tabs",
            "browser_devtools",
            "browser_cdp",
            PhoneControlHandler.BROWSER_TABS,
        ),
        real(
            "browser_switch_tab",
            "browser_devtools",
            "browser_cdp",
            PhoneControlHandler.BROWSER_SWITCH_TAB,
        ),
        real(
            "browser_close_tab",
            "browser_devtools",
            "browser_cdp",
            PhoneControlHandler.BROWSER_CLOSE_TAB,
        ),
        real(
            "browser_network",
            "browser_devtools",
            "browser_cdp",
            PhoneControlHandler.BROWSER_NETWORK,
        ),
        real(
            "browser_console",
            "browser_devtools",
            "browser_cdp",
            PhoneControlHandler.BROWSER_CONSOLE,
        ),
        unavailable("list_app_integrations", "system_query", "android_app_api"),
        unavailable("setup_app_integration", "app_and_task_control", "android_app_api"),
        unavailable("app_integration_status", "system_query", "android_app_api"),
        unavailable("read_app_integration_docs", "system_query", "android_app_api"),
        unavailable("remove_app_integration", "app_and_task_control", "android_app_api"),
    )

    val byName: Map<String, PhoneControlToolSpec> = specs.associateBy(PhoneControlToolSpec::name)

    init {
        require(byName.size == specs.size) { "Phone Control tool registry names must be unique" }
    }

    fun resolve(name: String, arguments: JsonObject): PhoneControlToolSpec? {
        val spec = byName[name] ?: return null
        if (name != "act") return spec
        val verb = (arguments["verb"] as? JsonPrimitive)
            ?.contentOrNull
            ?.lowercase(Locale.ROOT)
        return if (verb == "fill") {
            spec.copy(
                capability = "ui.text_edit",
                providerIds = listOf("accessibility"),
            )
        } else {
            spec
        }
    }

    private fun real(
        name: String,
        capability: String,
        provider: String,
        handler: PhoneControlHandler,
    ) = PhoneControlToolSpec(
        name = name,
        capability = capability,
        providerIds = listOf(provider),
        unavailableState = CapabilityState.UNAVAILABLE,
        handler = handler,
    )

    private fun realWithProviders(
        name: String,
        capability: String,
        providers: List<String>,
        handler: PhoneControlHandler,
        dependencyProviders: Set<String> = emptySet(),
    ) = PhoneControlToolSpec(
        name = name,
        capability = capability,
        providerIds = providers,
        dependencyProviderIds = dependencyProviders,
        unavailableState = CapabilityState.UNAVAILABLE,
        handler = handler,
    )

    private fun unavailable(
        name: String,
        capability: String,
        provider: String,
        requiredUserStep: String? = null,
    ) = PhoneControlToolSpec(
        name = name,
        capability = capability,
        providerIds = listOf(provider),
        unavailableState = CapabilityState.UNAVAILABLE,
        requiredUserStep = requiredUserStep,
    )

    private fun unsupported(
        name: String,
        capability: String,
        provider: String,
    ) = PhoneControlToolSpec(
        name = name,
        capability = capability,
        providerIds = listOf(provider),
        unavailableState = CapabilityState.UNSUPPORTED,
    )

}

private val POINTER_EFFECT_PROVIDERS = listOf(
    "accessibility",
    "sgt_adb_bridge",
    "shizuku_shell",
    "root_bridge",
)
