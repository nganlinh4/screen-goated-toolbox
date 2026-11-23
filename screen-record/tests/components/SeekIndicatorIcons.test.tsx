import { render } from "@testing-library/react";
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { SeekIndicator } from "@/components/VideoPreview";
import { KeyboardDoubleArrowLeft, KeyboardDoubleArrowRight } from "@/components/ui/MaterialIcon";

function pathOf(element: React.ReactElement) {
  const { container, unmount } = render(element);
  const d = container.querySelector("path")?.getAttribute("d");
  unmount();
  return d;
}

function tsxFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) return tsxFiles(full);
    return name.endsWith(".tsx") ? [full] : [];
  });
}

describe("seek indicator icons", () => {
  it.each([
    ["left", KeyboardDoubleArrowLeft],
    ["right", KeyboardDoubleArrowRight],
  ] as const)("draws the %s arrow from the Material Symbols catalog", (dir, Icon) => {
    const { container } = render(<SeekIndicator dir={dir} showKey={1} />);
    const arrow = container.querySelector(".seek-indicator-arrow path");
    expect(arrow?.getAttribute("d")).toBe(pathOf(<Icon />));
  });

  it("keeps literal icon path data out of components", () => {
    const srcRoot = path.resolve(import.meta.dirname, "../../src");
    const iconCatalog = path.join(srcRoot, "components", "ui", "MaterialIcon.tsx");
    const offenders = tsxFiles(srcRoot)
      .filter((file) => file !== iconCatalog)
      .filter((file) => /<path\s[^>]*\bd="M[\d.]/.test(readFileSync(file, "utf8")));
    expect(offenders).toEqual([]);
  });
});
