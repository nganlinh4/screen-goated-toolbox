import { readFileSync } from "node:fs";
import type { Page } from "@playwright/test";

export async function syntheticVideoUrl(page: Page, size: "1280x720" | "1920x1080") {
  const fixture = new URL(`./synthetic-${size}.webm`, import.meta.url);
  const base64 = readFileSync(fixture, "base64");
  return page.evaluate((encoded) => {
    const bytes = Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0));
    return URL.createObjectURL(new Blob([bytes], { type: "video/webm" }));
  }, base64);
}
