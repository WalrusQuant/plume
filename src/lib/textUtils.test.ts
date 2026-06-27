import { describe, it, expect } from "vitest";
import { stripWrappingFence } from "$lib/textUtils";

describe("stripWrappingFence", () => {
  it("strips a simple fence wrapping the entire text", () => {
    const fenced = "```\nhello world\n```";
    expect(stripWrappingFence(fenced)).toBe("hello world");
  });

  it("strips a fence with a language tag", () => {
    const fenced = "```markdown\n# Title\n\nBody text.\n```";
    expect(stripWrappingFence(fenced)).toBe("# Title\n\nBody text.");
  });

  it("strips a fence with arbitrary language tag", () => {
    expect(stripWrappingFence("```javascript\nconst x = 1;\n```")).toBe("const x = 1;");
  });

  it("returns unfenced text unchanged", () => {
    expect(stripWrappingFence("just plain text")).toBe("just plain text");
  });

  it("does NOT strip when the fence wraps only part of the text", () => {
    const partial = "intro text\n```\ncode\n```\nmore text";
    expect(stripWrappingFence(partial)).toBe(partial);
  });

  it("handles multi-line content inside the fence", () => {
    const fenced = "```\nline one\nline two\nline three\n```";
    expect(stripWrappingFence(fenced)).toBe("line one\nline two\nline three");
  });

  it("trims surrounding whitespace before matching", () => {
    const fenced = "  \n```\ncontent\n```\n  ";
    expect(stripWrappingFence(fenced)).toBe("content");
  });

  it("returns the text unchanged for an empty string", () => {
    expect(stripWrappingFence("")).toBe("");
  });
});
