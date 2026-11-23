import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";

const root = new URL("../../", import.meta.url);
const { cases: fixtures } = JSON.parse(readFileSync(new URL("parity-fixtures/live-translate/word-diff.json", root), "utf8"));
const sources = [
  ["Windows", "src/overlay/html_components/js_logic.rs", true],
  ["Android", "mobile/androidApp/src/main/assets/realtime_overlay/logic.js", false],
];

function functionBetween(source, name, next) {
  const start = source.indexOf(`function ${name}(`);
  const end = source.indexOf(`function ${next}(`, start);
  assert.ok(start >= 0 && end > start, `missing ${name}`);
  return source.slice(start, end);
}

for (const [platform, path, rustFormat] of sources) {
  test(`${platform} word diff preserves shared text cases`, () => {
    let source = readFileSync(new URL(path, root), "utf8");
    if (rustFormat) source = source.replaceAll("{{", "{").replaceAll("}}", "}");
    const functions = [
      functionBetween(source, "createChunk", "appendClassifiedText"),
      functionBetween(source, "appendClassifiedText", "rebuildStaticText"),
      functionBetween(source, "appendSequentialDiffText", "tokenizeForDiff"),
      functionBetween(source, "tokenizeForDiff", "canAnimateWordDiff"),
      functionBetween(source, "canAnimateWordDiff", "renderCommitAdvance"),
    ].join("\n");
    const document = { createElement: () => ({ style: {} }) };
    const api = runInNewContext(`${functions}; ({ tokenizeForDiff, appendSequentialDiffText, canAnimateWordDiff })`, { document });
    for (const { text, tokens } of fixtures) {
      assert.deepEqual(Array.from(api.tokenizeForDiff(text)), tokens);
      const pieces = [];
      api.appendSequentialDiffText({ appendChild: (piece) => pieces.push(piece) }, text, 0, Math.floor(text.length / 2));
      assert.equal(pieces.map((piece) => piece.textContent).join(""), text);
    }
    assert.equal(api.canAnimateWordDiff("one word", "one says"), true);
    assert.equal(api.canAnimateWordDiff("one word", "one !"), false);
  });
}
