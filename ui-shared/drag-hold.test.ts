import assert from "node:assert/strict";
import test from "node:test";
import { DragHold } from "./drag-hold.ts";

test("host echoes do not move a control while the user drags it", () => {
  const hold = new DragHold<number>();
  assert.equal(hold.present(40), 40);
  hold.hold(70);
  assert.equal(hold.active, true);
  // An echo of an earlier step of the same drag.
  assert.equal(hold.present(50), 70);
  hold.hold(80);
  assert.equal(hold.present(70), 80);
});

test("host state wins again once the gesture ends", () => {
  const hold = new DragHold<number>();
  hold.hold(80);
  hold.release();
  assert.equal(hold.active, false);
  assert.equal(hold.present(80), 80);
  assert.equal(hold.present(30), 30);
});
