import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { inputValidationPrimaryAction, runInputValidation } from "./input-validation-flow.ts";
import type { InputValidationError } from "./input-validation-flow.ts";

const fixture = JSON.parse(readFileSync(new URL("../../parity-fixtures/image-to-svg/input-contract.json", import.meta.url), "utf8"));

for (const [error, expected] of Object.entries<string>(fixture.errors)) {
  test(`shared validation error ${error} offers ${expected}`, async () => {
    const shown: InputValidationError[] = [];
    let checks = 0, chooses = 0;
    const accepted = await runInputValidation(
      async () => {
        checks++;
        if (checks > 1) return { error: null };
        if (error === "validation_unavailable") throw new Error("bridge unavailable");
        return { error };
      },
      async (reported) => { shown.push(reported); return inputValidationPrimaryAction(reported); },
      () => { chooses++; },
      new AbortController().signal,
      () => true,
    );
    assert.deepEqual(shown, [error]);
    assert.equal(inputValidationPrimaryAction(error as InputValidationError), expected);
    // Retry re-checks the same image; choose hands off to the picker without submitting.
    assert.equal(accepted, expected === "retry");
    assert.equal(chooses, expected === "choose" ? 1 : 0);
    assert.equal(checks, expected === "retry" ? 2 : 1);
  });
}
