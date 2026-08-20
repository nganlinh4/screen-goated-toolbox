import type { JobStatus } from "./types";

export function savedResultFiles(result?: JobStatus): string[] {
  return [result?.downloadName, result?.outputName].filter(
    (name, index, names): name is string => Boolean(name) && names.indexOf(name) === index,
  );
}

export function retainPublishedDownload(
  previous: JobStatus | undefined,
  next: JobStatus,
): JobStatus {
  if (
    next.downloadPath
    || !previous?.downloadPath
    || !previous.downloadName
    || next.outputPath !== previous.outputPath
  ) return next;
  return {
    ...next,
    downloadPath: previous.downloadPath,
    downloadName: previous.downloadName,
  };
}
