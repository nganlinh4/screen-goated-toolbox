import assert from "node:assert/strict";
import test from "node:test";
import { beginSvgSave, noteSvgEdit, settleSavedSvg, stepEditHistory } from "./svg-edit-history.ts";
import type { Item, SvgEditDelta } from "./types.ts";

const paint = (after: string): SvgEditDelta => ({
  kind: "paint", shapePath: [0], property: "fill", before: "", after,
});

function editedItem(): Item {
  const item = { id: "a", svgText: "<svg>v1</svg>", dirty: true, undoStack: [paint("red")], undoBytes: 10 } as Item;
  noteSvgEdit(item);
  return item;
}

test("a save with no edit in flight marks the item clean", () => {
  const item = editedItem();
  const save = beginSvgSave(item);
  assert.equal(settleSavedSvg(item, "<svg>saved</svg>", save), true);
  assert.equal(item.dirty, false);
  assert.equal(item.svgText, "<svg>saved</svg>");
  assert.deepEqual(item.undoStack, []);
  assert.equal(item.undoBytes, 0);
});

test("an edit made while the save was in flight stays unsaved and undoable", () => {
  const item = editedItem();
  const save = beginSvgSave(item);
  // The user recolours another shape before the host confirms the write.
  item.undoStack!.push(paint("blue"));
  item.svgText = "<svg>v2</svg>";
  noteSvgEdit(item);

  assert.equal(settleSavedSvg(item, "<svg>v1</svg>", save), false);
  assert.equal(item.dirty, true);
  assert.equal(item.saveError, false);
  assert.equal(item.svgText, "<svg>v2</svg>");
  assert.equal(item.undoStack!.length, 2);
});

function svgRoot(shape?: object) {
  return { children: { item: (index: number) => (index === 0 ? shape : null) ?? null } } as unknown as SVGSVGElement;
}

function paintedShape() {
  const style = new Map<string, string>([["fill", "red"]]);
  return {
    style,
    shape: {
      children: { item: () => null },
      style: {
        setProperty: (name: string, value: string) => style.set(name, value),
        removeProperty: (name: string) => style.delete(name),
      },
    },
  };
}

test("an undo step that no longer applies stays undoable instead of being dropped", () => {
  const item = editedItem();
  const result = stepEditHistory(svgRoot(), item, false);
  assert.equal(result.applied, false);
  assert.equal(item.undoStack!.length, 1);
  assert.equal(item.undoBytes, 10);
  assert.deepEqual(item.redoStack ?? [], []);
});

test("an applied undo moves the step to redo and back", () => {
  const item = editedItem();
  const { style, shape } = paintedShape();
  const undone = stepEditHistory(svgRoot(shape), item, false);
  assert.equal(undone.applied, true);
  assert.equal(style.has("fill"), false);
  assert.equal(item.undoStack!.length, 0);
  assert.equal(item.redoStack!.length, 1);
  assert.ok(item.redoBytes! > 0);
  const redone = stepEditHistory(svgRoot(shape), item, true);
  assert.equal(redone.applied, true);
  assert.equal(style.get("fill"), "red");
  assert.equal(item.undoStack!.length, 1);
  assert.equal(item.redoStack!.length, 0);
  assert.equal(item.redoBytes, 0);
});
