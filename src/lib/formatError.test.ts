import { describe, it, expect } from "vitest";
import { formatError } from "$lib/formatError";

describe("formatError", () => {
  it("extracts .message from a Rust-style { kind, message } object", () => {
    const rustErr = { kind: "Io", message: "disk full" };
    expect(formatError(rustErr)).toBe("disk full");
  });

  it("returns .message from a native Error", () => {
    expect(formatError(new Error("boom"))).toBe("boom");
  });

  it("returns a string error as-is", () => {
    expect(formatError("something broke")).toBe("something broke");
  });

  it("falls back to String() for unknown shapes", () => {
    expect(formatError(42)).toBe("42");
    expect(formatError(null)).toBe("null");
  });

  it("does not crash on objects without a message field", () => {
    expect(formatError({ foo: "bar" })).toBe("[object Object]");
  });

  it("handles { kind, message } where message is non-string", () => {
    const err = { kind: "Test", message: 123 };
    expect(formatError(err)).toBe("123");
  });
});
