# Android Agent Rules

Root `AGENTS.md` applies. Keep Android platform code thin.

## Phone Control gate

Before Phone Control work, read:

1. `../docs/COMPUTER_CONTROL_DEVELOPMENT.md` — canonical rules.
2. `../.claude/parity/phone-control.md` — Android contract.
3. `../.claude/skills/enforce-mobile-parity/SKILL.md` — workflow.
4. The affected fixture under `../parity-fixtures/phone-control/`.

Non-negotiable:

- Model owns language, planning, tool choice, and semantic completion.
- Give every normal turn the full stable catalog. Unavailable tools return typed capability state.
- Code gates structure and effects only. Never gate or reroute from words, language, app, site, person, task, incident, OEM, model, resolution, or emulator.
- Bind audio, jobs, observations, and targets to their generation. Reject stale or ambiguous identity; never guess.
- Report effect truthfully. Uncertain or unverified effects stay typed as such.
- One user turn gives at most one final response, then idle. Never synthesize continuation turns.
- Windows behavior is canonical. Android may branch only on probed capability and platform state.
- Update parity text and a shared fixture before behavior. Fix shared architecture when glue starts to drift.

Verify both `full` and `play` variants. Use real-device evidence for platform behavior.
