# Feature Parity Template

Use this file as the starting point for any mobile feature that must match Windows.

## Canonical Source
- Windows entrypoints:
- Supporting state/logic:
- UI/output owners:
- If Windows uses HTML/CSS/JS/WebView, name that web surface explicitly as canonical:

## Behavior Contract
- User-visible flow:
- State model:
- Transition rules:
- Output contract:
- If Windows uses a web surface, note whether mobile is sharing/extracting that surface directly or verbatim-porting it with a thin bridge/shim:

## Failure And Recovery
- Permission/runtime failures:
- Timeout/retry behavior:
- Stop/reset behavior:

## Fixtures
- Shared fixtures:
- Platform-specific tests:

## Deviations
- Default: none
