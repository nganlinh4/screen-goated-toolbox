import { invoke } from "@/lib/ipc";

interface SaveCurrentFrameOptions {
  canvas: HTMLCanvasElement | null;
  currentTime: number;
  notificationTitle: string;
  projectName?: string | null;
}

interface SavedFrameResult {
  savedPath?: string;
}

function sanitizeFileStem(value: string): string {
  const cleaned = value
    .replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[. ]+$/g, "")
    .slice(0, 96);
  return cleaned || "recording";
}

function formatFrameTimestamp(seconds: number): string {
  const milliseconds = Math.max(0, Math.round(seconds * 1000));
  const hours = Math.floor(milliseconds / 3_600_000);
  const minutes = Math.floor((milliseconds % 3_600_000) / 60_000);
  const secs = Math.floor((milliseconds % 60_000) / 1000);
  const millis = milliseconds % 1000;
  return [hours, minutes, secs]
    .map((value) => value.toString().padStart(2, "0"))
    .join("-") + `-${millis.toString().padStart(3, "0")}`;
}

export function buildFrameFileName(projectName: string | null | undefined, currentTime: number): string {
  return `${sanitizeFileStem(projectName || "recording")}-frame-${formatFrameTimestamp(currentTime)}.png`;
}

export function canvasToPngDataUrl(canvas: HTMLCanvasElement): Promise<string> {
  if (canvas.width < 1 || canvas.height < 1) {
    return Promise.reject(new Error("The preview frame is empty"));
  }

  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (!blob) {
        reject(new Error("Could not encode the preview frame"));
        return;
      }

      const reader = new FileReader();
      reader.onerror = () => reject(new Error("Could not read the encoded preview frame"));
      reader.onload = () => {
        if (typeof reader.result === "string") {
          resolve(reader.result);
        } else {
          reject(new Error("Could not read the encoded preview frame"));
        }
      };
      reader.readAsDataURL(blob);
    }, "image/png");
  });
}

export async function saveCurrentFrame({
  canvas,
  currentTime,
  notificationTitle,
  projectName,
}: SaveCurrentFrameOptions): Promise<string> {
  if (!canvas) throw new Error("The preview frame is not ready");

  const dataUrl = await canvasToPngDataUrl(canvas);
  const result = await invoke<SavedFrameResult>("save_current_frame", {
    dataUrl,
    defaultFileName: buildFrameFileName(projectName, currentTime),
    notificationTitle,
  });
  if (!result?.savedPath) throw new Error("The frame was not saved");
  return result.savedPath;
}
