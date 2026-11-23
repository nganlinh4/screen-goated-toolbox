import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import "@testing-library/jest-dom/vitest";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MaterialSymbol } from "../src/MaterialSymbol";

const srcDir = path.resolve(__dirname, "../src");
const symbolDir = path.resolve(__dirname, "../../ui-shared/material-symbols");
const catalogPath = (file: string) =>
  /\sd="([^"]+)"/.exec(readFileSync(path.join(symbolDir, `${file}.svg`), "utf8"))![1];

describe("Material Symbols pipeline", () => {
  it("has no hand-pasted SVG path data in the app sources", () => {
    for (const file of readdirSync(srcDir).filter((name) => /\.tsx?$/.test(name))) {
      const source = readFileSync(path.join(srcDir, file), "utf8");
      expect(source, file).not.toMatch(/\sd="/);
    }
  });

  it("renders the shared catalog glyph", () => {
    const { container } = render(
      <>
        <MaterialSymbol name="close" className="h-3 w-3" />
        <MaterialSymbol name="graphicEq" />
      </>,
    );
    const paths = [...container.querySelectorAll("svg path")].map((node) => node.getAttribute("d"));
    expect(paths).toEqual([catalogPath("close"), catalogPath("graphic_eq")]);
    expect(container.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    expect(container.querySelector("svg")).toHaveClass("h-3", "w-3");
  });
});
