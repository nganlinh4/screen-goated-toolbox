---
name: gemini-api-dev
description: Build, debug, or review Gemini API integrations in this repository, including model selection, multimodal requests, tools, structured output, SDK use, and model migrations. Use whenever a change depends on current Gemini model capabilities or wire contracts.
---

# Gemini API Development

## Workflow

1. Read the existing call path and tests before changing a payload.
2. Read `catalog/model_catalog.json`; it is the repository model source of truth.
3. Check the current official [model list](https://ai.google.dev/gemini-api/docs/models), [deprecation table](https://ai.google.dev/gemini-api/docs/deprecations), and relevant API guide. Model IDs and preview lifetimes change too quickly to duplicate here.
4. Check the official [API-version matrix](https://ai.google.dev/gemini-api/docs/api-versions). Use the version already required by the feature unless the change explicitly migrates it.
5. Match the request to the selected model's documented capabilities. Never infer support from another Gemini family.
6. Update Windows, Android, catalog generation, parity fixtures, and tests together when the shared contract changes.

## Repository Map

- Model manifest: `catalog/model_catalog.json`
- Generated Windows view: `src/model_config.rs`
- Text API dispatch: `src/api/`
- Recorder Gemini paths: `src/overlay/screen_record/`
- Android catalog/runtime: `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/`
- Catalog workflow: `.claude/commands/manage-model-catalog.md`

## Rules

- Prefer the repository's existing official GenAI SDK or REST path. Do not add a second client stack without a concrete need.
- Do not hardcode a model list, shutdown date, quota, or capability in prose when the catalog or official documentation can own it.
- Preserve exact API casing and nesting; verify payloads with unit tests or captured sanitized requests.
- Keep API keys, service-account files, project IDs, and machine paths out of tracked files and logs.
- Treat preview aliases as volatile. Pin or migrate intentionally, then verify availability with the API/model catalog.
- For Live API work, also use `../gemini-live-api-dev/SKILL.md`.
