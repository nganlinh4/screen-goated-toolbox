#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1
STATIC_DECLARATION_COUNT = 62
ASSET_PATH = "phone_control/catalog.json"
PROMPT_ASSET_PATH = "phone_control/prompt_core.txt"
AUTHORITY_ASSET_PATH = "phone_control/authority-matrix.json"
ORB_ASSET_PATH = "phone_control/orb.html"
ORB_CONTRACT_ASSET_PATH = "phone_control/orb-contract.json"
PLATFORM_DEVICE_TOKEN = "{{PLATFORM_DEVICE}}"


def load_catalog(source: Path) -> dict[str, Any]:
    catalog = json.loads(source.read_text(encoding="utf-8"))
    if not isinstance(catalog, dict):
        raise ValueError("Phone Control catalog must be a JSON object")
    if set(catalog) != {"schemaVersion", "functionDeclarations"}:
        raise ValueError("Phone Control catalog has unsupported top-level fields")
    if catalog["schemaVersion"] != SCHEMA_VERSION:
        raise ValueError(
            f"Phone Control catalog schema must be {SCHEMA_VERSION}"
        )

    declarations = catalog["functionDeclarations"]
    if not isinstance(declarations, list):
        raise ValueError("functionDeclarations must be an array")
    if len(declarations) != STATIC_DECLARATION_COUNT:
        raise ValueError(
            f"expected {STATIC_DECLARATION_COUNT} declarations, got {len(declarations)}"
        )

    names: set[str] = set()
    for index, declaration in enumerate(declarations):
        if not isinstance(declaration, dict):
            raise ValueError(f"declaration {index} must be an object")
        if set(declaration) != {"name", "description", "parameters"}:
            raise ValueError(f"declaration {index} has unsupported fields")
        name = declaration["name"]
        description = declaration["description"]
        parameters = declaration["parameters"]
        if not isinstance(name, str) or not name:
            raise ValueError(f"declaration {index} has no name")
        if name in names:
            raise ValueError(f"duplicate declaration: {name}")
        names.add(name)
        if not isinstance(description, str) or not description.strip():
            raise ValueError(f"{name} has no description")
        if not isinstance(parameters, dict) or parameters.get("type") != "object":
            raise ValueError(f"{name} parameters must be an object schema")
        properties = parameters.get("properties")
        if not isinstance(properties, dict):
            raise ValueError(f"{name} parameters must define properties")
        required = parameters.get("required", [])
        if (
            not isinstance(required, list)
            or not all(isinstance(field, str) for field in required)
            or not set(required).issubset(properties)
        ):
            raise ValueError(f"{name} has invalid required fields")
    return catalog


def compact_json(catalog: dict[str, Any]) -> str:
    return json.dumps(catalog, ensure_ascii=False, separators=(",", ":"))


def load_prompt(source: Path) -> str:
    prompt = source.read_text(encoding="utf-8")
    if prompt.count(PLATFORM_DEVICE_TOKEN) != 1:
        raise ValueError("prompt core must contain one platform-device token")
    if not prompt.endswith("\n"):
        raise ValueError("prompt core must end with a newline")
    return prompt


def load_authority_matrix(source: Path) -> dict[str, Any]:
    matrix = json.loads(source.read_text(encoding="utf-8"))
    if not isinstance(matrix, dict) or matrix.get("feature") != "phone-control":
        raise ValueError("authority matrix must describe phone-control")
    distribution = matrix.get("distribution")
    if not isinstance(distribution, dict):
        raise ValueError("authority matrix must define distribution parity")
    if set(distribution.get("agentFlavors", [])) != {"play", "full"}:
        raise ValueError("Phone Control must ship in both play and full flavors")
    if (
        distribution.get("behavior") != "identical"
        or distribution.get("catalogAndRuntimeMustMatch") is not True
    ):
        raise ValueError("Phone Control flavor behavior and runtime must stay identical")
    providers = matrix.get("providers")
    routes = matrix.get("routes")
    states = matrix.get("capabilityStates")
    if not isinstance(providers, list) or not providers:
        raise ValueError("authority matrix must define providers")
    if not isinstance(routes, list) or not routes:
        raise ValueError("authority matrix must define routes")
    if not isinstance(states, list) or not states:
        raise ValueError("authority matrix must define capability states")
    provider_ids = [provider.get("id") for provider in providers if isinstance(provider, dict)]
    if len(provider_ids) != len(providers) or any(not value for value in provider_ids):
        raise ValueError("authority providers must have ids")
    if len(set(provider_ids)) != len(provider_ids):
        raise ValueError("authority provider ids must be unique")
    for route in routes:
        if not isinstance(route, dict) or not route.get("capability"):
            raise ValueError("authority routes must name a capability")
        candidates = route.get("providers")
        if not isinstance(candidates, list) or not candidates:
            raise ValueError("authority routes must name providers")
        unknown = set(candidates) - set(provider_ids)
        if unknown:
            raise ValueError(f"authority route references unknown providers: {sorted(unknown)}")
    validate_surface_semantics(matrix)
    return matrix


def validate_surface_semantics(matrix: dict[str, Any]) -> None:
    surface = matrix.get("surfaceSemantics")
    if not isinstance(surface, dict):
        raise ValueError("authority matrix must define Android surface semantics")
    if surface.get("listWindowsScope") != "current_interactive_surfaces_only":
        raise ValueError("list_windows must be scoped to current interactive surfaces")

    ownership = surface.get("ownership")
    if (
        not isinstance(ownership, dict)
        or ownership.get("controllerOwnedScope")
        != "accessibility_overlay_or_same_package_non_application_window"
        or ownership.get("samePackageApplicationWindow") != "ordinary_targetable_surface"
        or ownership.get("ordinaryToolsBlockControllerOverlay") is not True
    ):
        raise ValueError("controller ownership must exclude ordinary same-package activities")

    token = surface.get("token")
    if not isinstance(token, dict):
        raise ValueError("surface semantics must define observation-bound tokens")
    if (
        token.get("lifetime") != "current_observation_generation"
        or token.get("staleResult") != "stale_target"
        or token.get("staleEffect") != "proven_no_effect"
        or token.get("exactResolutionRequired") is not True
    ):
        raise ValueError("surface tokens must resolve exactly and fail stale with no effect")

    resolution = surface.get("nameResolution")
    if not isinstance(resolution, dict):
        raise ValueError("surface semantics must define exact name resolution")
    if (
        resolution.get("zeroMatches") != "target_not_found"
        or resolution.get("multipleMatches") != "ambiguous_target"
        or resolution.get("firstMatchFallback") is not False
    ):
        raise ValueError("surface name resolution must reject missing or ambiguous matches")

    focus = surface.get("focusWindow")
    text = surface.get("textTargeting")
    navigation = surface.get("systemNavigationKeys")
    stale_recovery = surface.get("staleRecovery")
    invalidation = surface.get("observationInvalidation")
    paste = surface.get("pasteArtifactTargeting")
    minimize = surface.get("minimizeWindow")
    move = surface.get("moveWindow")
    resize = surface.get("resizeWindow")
    visual = surface.get("visualObservation")
    if not all(isinstance(value, dict) for value in (focus, minimize, move, resize)):
        raise ValueError("surface semantics must define every window-tool boundary")
    if focus.get("successPostcondition") != (
        "fresh_observation_proves_requested_package_active_and_focused"
    ):
        raise ValueError("focus_window must require a verified fresh postcondition")
    if (
        not isinstance(text, dict)
        or text.get("tools") != ["type_text", "key_combination"]
        or text.get("target") != "current_surface_token"
        or text.get("nodeIdMayReplaceSurfaceToken") is not False
    ):
        raise ValueError("only explicit text tools use the current surface token")
    if (
        not isinstance(navigation, dict)
        or navigation.get("tool") != "key_combination"
        or navigation.get("target") != "current_surface_token"
        or navigation.get("baselineProvider") != "accessibility"
        or navigation.get("focusedEditorRequired") is not False
        or navigation.get("exactForegroundSurfaceLeaseRequired") is not True
        or navigation.get("foregroundSurfaceScope") != "current_non_controller_platform_window"
        or navigation.get("pointerGeometryRequired") is not False
        or navigation.get("inactiveHigherWindowBlocksDispatch") is not False
        or navigation.get("activeOsOwnedUserStepBlocksDispatch") is not True
        or navigation.get("singleKeyOnly") is not True
        or navigation.get("keys") != [
            "back",
            "home",
            "recents",
            "notifications",
            "quick_settings",
        ]
    ):
        raise ValueError("Android system navigation keys must remain exact Accessibility actions")
    if (
        not isinstance(stale_recovery, dict)
        or stale_recovery.get("automaticTargetRebinding") is not False
        or stale_recovery.get("provenNoEffectReceiptAttachesFreshObservation") is not True
        or stale_recovery.get("attachedObservationContainsCurrentSurfaceTargets") is not True
        or stale_recovery.get("retryUsesOnlyAttachedGeneration") is not True
    ):
        raise ValueError("stale target recovery must attach fresh evidence without rebinding")
    if (
        not isinstance(invalidation, dict)
        or invalidation.get("backgroundVisualCaptureMayReplaceActionLeases") is not False
        or invalidation.get("windowTopologyAndUserMutationEventsInvalidateImmediately") is not True
        or invalidation.get("semanticOnlyAccessibilityChurnInvalidatesImmediately") is not False
        or invalidation.get("everyMutationRevalidatesLiveTargetFingerprint") is not True
        or invalidation.get("hardEventDuringCaptureResult") != "stale_frame"
        or invalidation.get("staleVisualCaptureRetriesAreBounded") is not True
    ):
        raise ValueError("observation invalidation must separate leases from visual streaming")
    if (
        not isinstance(paste, dict)
        or paste.get("target") != "fresh_unique_focused_editor"
        or paste.get("surfaceTokenParameter") is not False
        or paste.get("artifactBodyStaysLocal") is not True
    ):
        raise ValueError("paste_artifact must use fresh unique focus without a target parameter")
    if minimize.get("supportedScope") != "sole_active_fullscreen_app":
        raise ValueError("minimize_window must keep its narrow Android scope")
    if any(
        value.get("arbitraryAndroidGeometry") != "unsupported_on_surface"
        for value in (move, resize)
    ):
        raise ValueError("arbitrary Android move/resize must stay unsupported")
    if (
        not isinstance(visual, dict)
        or visual.get("normalFrame") != "current_view_with_numbered_6x5_grid"
        or visual.get("look") != "clean_current_view_frame_for_same_live_model"
        or visual.get("staleResult") != "stale_frame"
        or visual.get("overlayPolicy") != (
            "window_capture_excludes_controller_overlay_without_visible_mutation_when_available"
        )
    ):
        raise ValueError("visual observations must use exact generation-bound clean/grid frames")


def load_orb_contract(source: Path, renderer_source: Path) -> tuple[dict[str, Any], str]:
    contract = json.loads(source.read_text(encoding="utf-8"))
    renderer = renderer_source.read_bytes().decode("utf-8")
    expected_fields = {
        "schemaVersion",
        "feature",
        "canonicalAsset",
        "androidAsset",
        "invariants",
        "stateLabels",
        "toolStateGroups",
        "defaultToolState",
        "scrollDirectionIcons",
        "captureRoutes",
    }
    if not isinstance(contract, dict) or set(contract) != expected_fields:
        raise ValueError("orb contract has unsupported top-level fields")
    if contract["schemaVersion"] != 1 or contract["feature"] != "phone-control-orb":
        raise ValueError("orb contract identity is invalid")
    if contract["canonicalAsset"] != "src/overlay/computer_control/orb/orb.html":
        raise ValueError("orb contract must point at the Windows canonical renderer")
    if contract["androidAsset"] != ORB_ASSET_PATH:
        raise ValueError("orb contract Android asset path is invalid")

    invariants = contract["invariants"]
    if not isinstance(invariants, dict) or any(value is not True for key, value in invariants.items() if key.startswith("same")):
        raise ValueError("orb renderer parity invariants must remain enabled")
    if invariants.get("captureDoesNotMutateVisibleOverlayWhenWindowCaptureIsAvailable") is not True:
        raise ValueError("window-scoped capture must not mutate the visible orb")
    if invariants.get("freshReceiptPostconditionDoesNotPublishTransientWarning") is not True:
        raise ValueError("fresh receipts must not publish transient reconciliation warnings")
    if (
        invariants.get("sameCaptionRenderer") is not True
        or invariants.get("sameIncrementalCaptionMotion") is not True
    ):
        raise ValueError("Android must use the canonical incremental caption renderer")
    if invariants.get("androidVisualHost") != (
        "trusted_accessibility_overlay_with_non_obscuring_fallback"
    ):
        raise ValueError("Android orb host must preserve touch-through rendering")
    if invariants.get("visualOverlayDoesNotConsumeUnderlyingTouches") is not True:
        raise ValueError("Android orb visuals must not consume underlying touches")

    states = contract["stateLabels"]
    if not isinstance(states, list) or not states or len(states) != len(set(states)):
        raise ValueError("orb state labels must be a unique non-empty array")
    for state in states:
        if not isinstance(state, str) or f"label:'{state}'" not in renderer:
            raise ValueError(f"orb renderer is missing state: {state}")

    groups = contract["toolStateGroups"]
    if not isinstance(groups, dict) or not groups:
        raise ValueError("orb contract must define tool-state groups")
    seen_tools: set[str] = set()
    for state, tools in groups.items():
        if state not in states or not isinstance(tools, list) or not tools:
            raise ValueError(f"invalid orb tool-state group: {state}")
        for tool in tools:
            if not isinstance(tool, str) or not tool or tool in seen_tools:
                raise ValueError(f"duplicate or invalid orb tool mapping: {tool}")
            seen_tools.add(tool)
    if contract["defaultToolState"] not in states:
        raise ValueError("default orb tool state is not a canonical renderer state")

    direction_icons = contract["scrollDirectionIcons"]
    if not isinstance(direction_icons, dict) or set(direction_icons) != {"up", "down", "left", "right"}:
        raise ValueError("orb scroll directions must cover up, down, left, and right")
    for icon in direction_icons.values():
        if not isinstance(icon, str) or f"{icon}:" not in renderer:
            raise ValueError(f"orb renderer is missing directional icon: {icon}")
    return contract, renderer


def write_if_changed(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists() or path.read_text(encoding="utf-8") != content:
        path.write_text(content, encoding="utf-8", newline="\n")


def write_bytes_if_changed(path: Path, content: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists() or path.read_bytes() != content:
        path.write_bytes(content)


def generate(
    catalog: dict[str, Any],
    prompt: str,
    kotlin_output: Path,
    asset_output: Path,
    prompt_output: Path,
    authority_matrix: dict[str, Any],
    authority_output: Path,
    orb_contract: dict[str, Any],
    orb_renderer: str,
    orb_contract_output: Path,
    orb_output: Path,
) -> None:
    digest = hashlib.sha256(compact_json(catalog).encode("utf-8")).hexdigest()
    prompt_digest = hashlib.sha256(prompt.encode("utf-8")).hexdigest()
    orb_digest = hashlib.sha256(orb_renderer.encode("utf-8")).hexdigest()
    tool_state_arms = []
    for state, tools in orb_contract["toolStateGroups"].items():
        quoted_tools = ", ".join(json.dumps(tool) for tool in tools)
        tool_state_arms.append(f"        {quoted_tools} -> {json.dumps(state)}")
    tool_state_when = "\n".join(tool_state_arms)
    direction_arms = "\n".join(
        f"        {json.dumps(direction)} -> {json.dumps(icon)}"
        for direction, icon in orb_contract["scrollDirectionIcons"].items()
    )
    kotlin = f"""package dev.screengoated.toolbox.mobile.phonecontrol

// Generated from the Windows-owned Phone Control catalog and prompt core. Do not edit.
internal object GeneratedPhoneControlContract {{
    const val SCHEMA_VERSION = {SCHEMA_VERSION}
    const val STATIC_DECLARATION_COUNT = {STATIC_DECLARATION_COUNT}
    const val CATALOG_SHA256 = "{digest}"
    const val CATALOG_ASSET_PATH = "{ASSET_PATH}"
    const val PROMPT_CORE_SHA256 = "{prompt_digest}"
    const val PROMPT_CORE_ASSET_PATH = "{PROMPT_ASSET_PATH}"
    const val AUTHORITY_MATRIX_ASSET_PATH = "{AUTHORITY_ASSET_PATH}"
    const val ORB_ASSET_PATH = "{ORB_ASSET_PATH}"
    const val ORB_CONTRACT_ASSET_PATH = "{ORB_CONTRACT_ASSET_PATH}"
    const val ORB_SHA256 = "{orb_digest}"
    const val ORB_STATE_IDLE = "idle"
    const val ORB_STATE_THINKING = "thinking"
    const val ORB_STATE_RESPONDING = "responding"
    const val ORB_STATE_DONE = "done"
    const val ORB_STATE_ERROR = "error"
    const val ORB_STATE_SCROLL = "scroll"
    const val PLATFORM_DEVICE_TOKEN = "{PLATFORM_DEVICE_TOKEN}"

    fun orbStateForTool(name: String): String = when (name) {{
{tool_state_when}
        else -> {json.dumps(orb_contract["defaultToolState"])}
    }}

    fun scrollIconForDirection(direction: String?): String = when (direction) {{
{direction_arms}
        else -> {json.dumps(orb_contract["scrollDirectionIcons"]["down"])}
    }}
}}
"""
    asset = json.dumps(catalog, ensure_ascii=False, indent=2) + "\n"
    write_if_changed(kotlin_output, kotlin)
    write_if_changed(asset_output, asset)
    write_if_changed(prompt_output, prompt)
    write_if_changed(
        authority_output,
        json.dumps(authority_matrix, ensure_ascii=False, indent=2) + "\n",
    )
    write_if_changed(
        orb_contract_output,
        json.dumps(orb_contract, ensure_ascii=False, indent=2) + "\n",
    )
    write_bytes_if_changed(orb_output, orb_renderer.encode("utf-8"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog-source", required=True)
    parser.add_argument("--prompt-source", required=True)
    parser.add_argument("--authority-source", required=True)
    parser.add_argument("--orb-contract-source", required=True)
    parser.add_argument("--orb-source", required=True)
    parser.add_argument("--kotlin-output")
    parser.add_argument("--asset-output")
    parser.add_argument("--prompt-output")
    parser.add_argument("--authority-output")
    parser.add_argument("--orb-contract-output")
    parser.add_argument("--orb-output")
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    outputs = (
        args.kotlin_output,
        args.asset_output,
        args.prompt_output,
        args.authority_output,
        args.orb_contract_output,
        args.orb_output,
    )
    if not args.validate_only and not all(outputs):
        raise SystemExit("all contract outputs are required")

    catalog = load_catalog(Path(args.catalog_source))
    prompt = load_prompt(Path(args.prompt_source))
    authority_matrix = load_authority_matrix(Path(args.authority_source))
    orb_contract, orb_renderer = load_orb_contract(
        Path(args.orb_contract_source),
        Path(args.orb_source),
    )
    if not args.validate_only:
        generate(
            catalog,
            prompt,
            Path(args.kotlin_output),
            Path(args.asset_output),
            Path(args.prompt_output),
            authority_matrix,
            Path(args.authority_output),
            orb_contract,
            orb_renderer,
            Path(args.orb_contract_output),
            Path(args.orb_output),
        )


if __name__ == "__main__":
    main()
