import { isNarrationProjectLoading, narrationProjectEpoch } from './narrationProjectOwnership';

/** A successful load also provides a check for its exact activation. */
export type ImportProjectLoadResult = boolean | void | (() => boolean);
export type ImportStartOwner = (() => boolean) | null;
const importedOwners = new WeakMap<object, () => boolean>();

export function importProjectLoadCurrent(result: ImportProjectLoadResult): boolean {
  return result !== false && (typeof result !== 'function' || result());
}

export function rememberImportedProjectOwner(project: object, result: ImportProjectLoadResult): void {
  if (typeof result === 'function') importedOwners.set(project, result);
}

export function getImportedProjectOwner(project: object): (() => boolean) | undefined {
  return importedOwners.get(project);
}

export function importedProjectOwner(ref: React.MutableRefObject<string | null>, projectId: string): false | (() => boolean) {
  if (ref.current !== projectId) return false;
  const epoch = narrationProjectEpoch(ref);
  return () => ref.current === projectId && narrationProjectEpoch(ref) === epoch;
}

export function captureImportStartOwner(ref: React.MutableRefObject<string | null>): ImportStartOwner {
  if (isNarrationProjectLoading(ref)) return null;
  const projectId = ref.current;
  const epoch = narrationProjectEpoch(ref);
  return () => !isNarrationProjectLoading(ref) && ref.current === projectId && narrationProjectEpoch(ref) === epoch;
}

export function importStartOwnerCurrent(owner: ImportStartOwner | undefined): boolean {
  return owner !== null && (!owner || owner());
}
