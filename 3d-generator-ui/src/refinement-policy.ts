export function declaredRefinements(
  supportedActions: string[] | undefined,
  availableActions: string[] | undefined,
) {
  return new Set(supportedActions ?? availableActions ?? []);
}

export function hasDeclaredRefinements(
  supportedActions: string[] | undefined,
  availableActions: string[] | undefined,
) {
  return declaredRefinements(supportedActions, availableActions).size > 0;
}
