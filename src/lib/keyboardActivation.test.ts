import { describe, expect, it } from "vitest";
import { shouldActivateFromKey } from "./keyboardActivation";

describe("shouldActivateFromKey", () => {
  it.each(["Enter", " "])("activates a focused row with %j", (key) => {
    const row = {} as EventTarget;

    expect(shouldActivateFromKey({ key, target: row, currentTarget: row })).toBe(true);
  });

  it("does not consume spaces typed in a nested rename input", () => {
    const row = {} as EventTarget;
    const input = {} as EventTarget;

    expect(shouldActivateFromKey({ key: " ", target: input, currentTarget: row })).toBe(false);
  });
});
