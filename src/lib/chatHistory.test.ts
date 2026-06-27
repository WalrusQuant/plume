import { describe, it, expect } from "vitest";
import {
  capHistory,
  toApiMessages,
  deriveTitle,
  estimateTokens,
  DEFAULT_CHAT_TITLE,
  OPENROUTER_HISTORY_BUDGET,
} from "$lib/chatHistory";
import type { ChatMessage } from "$lib/api";

function msg(role: "user" | "assistant", content: string, rawContent?: unknown): ChatMessage {
  return { role, content, ...(rawContent ? { rawContent } : {}) };
}

// ---------------------------------------------------------------------------
// deriveTitle
// ---------------------------------------------------------------------------

describe("deriveTitle", () => {
  it("returns the default for empty/whitespace input", () => {
    expect(deriveTitle("")).toBe(DEFAULT_CHAT_TITLE);
    expect(deriveTitle("   \n\t ")).toBe(DEFAULT_CHAT_TITLE);
  });

  it("collapses internal whitespace", () => {
    expect(deriveTitle("hello    world\n\n  foo")).toBe("hello world foo");
  });

  it("truncates at 40 code points with an ellipsis", () => {
    const long = "a".repeat(50);
    const title = deriveTitle(long);
    expect(title).toHaveLength(41); // 40 chars + …
    expect(title.endsWith("…")).toBe(true);
  });

  it("keeps text under 40 chars as-is", () => {
    expect(deriveTitle("short title")).toBe("short title");
  });

  it("slices by code points, not UTF-16 units (emoji at boundary)", () => {
    // 41 emoji characters → should truncate to 40 + …
    const emojis = "😀".repeat(41);
    const title = deriveTitle(emojis);
    expect([...title]).toHaveLength(41); // 40 emoji + …
  });
});

// ---------------------------------------------------------------------------
// estimateTokens
// ---------------------------------------------------------------------------

describe("estimateTokens", () => {
  it("estimates ~4 chars per token (ceil)", () => {
    expect(estimateTokens("")).toBe(0);
    expect(estimateTokens("abcd")).toBe(1);
    expect(estimateTokens("abcde")).toBe(2);
    expect(estimateTokens("abcdefgh")).toBe(2);
  });
});

// ---------------------------------------------------------------------------
// toApiMessages
// ---------------------------------------------------------------------------

describe("toApiMessages", () => {
  it("passes alternating roles through unchanged", () => {
    const msgs = [
      msg("user", "hello"),
      msg("assistant", "hi there"),
      msg("user", "bye"),
    ];
    const out = toApiMessages(msgs);
    expect(out).toHaveLength(3);
    expect(out.map((m) => m.role)).toEqual(["user", "assistant", "user"]);
  });

  it("merges consecutive same-role messages with a blank line", () => {
    const msgs = [
      msg("user", "first"),
      msg("user", "second"),
      msg("assistant", "reply"),
    ];
    const out = toApiMessages(msgs);
    expect(out).toHaveLength(2);
    expect(out[0].content).toBe("first\n\nsecond");
    expect(out[1].content).toBe("reply");
  });

  it("does NOT merge a block-bearing turn (rawContent)", () => {
    const block = [{ type: "text", text: "summary" }];
    const msgs = [
      msg("assistant", "part 1", block),
      msg("assistant", "part 2"),
    ];
    const out = toApiMessages(msgs);
    expect(out).toHaveLength(2);
    expect(out[0].rawContent).toBe(block);
    expect(out[1].rawContent).toBeUndefined();
  });

  it("does NOT merge INTO a block-bearing turn", () => {
    const block = [{ type: "text", text: "summary" }];
    const msgs = [
      msg("assistant", "part 1"),
      msg("assistant", "part 2", block),
    ];
    const out = toApiMessages(msgs);
    expect(out).toHaveLength(2);
    expect(out[1].rawContent).toBe(block);
  });

  it("produces deep copies (mutating output doesn't affect input)", () => {
    const msgs = [msg("user", "hello")];
    const out = toApiMessages(msgs);
    out[0].content = "changed";
    expect(msgs[0].content).toBe("hello");
  });
});

// ---------------------------------------------------------------------------
// capHistory
// ---------------------------------------------------------------------------

describe("capHistory", () => {
  it("returns empty array for empty input", () => {
    expect(capHistory([], 1000)).toEqual([]);
  });

  it("keeps everything under the budget", () => {
    const msgs = [
      msg("user", "a".repeat(100), undefined),
      msg("assistant", "b".repeat(100)),
      msg("user", "c".repeat(100)),
    ];
    const out = capHistory(msgs, 1000);
    expect(out).toHaveLength(3);
  });

  it("drops oldest turns over the budget but always keeps the newest", () => {
    // each message ~250 tokens (1000 chars / 4)
    const msgs = [
      msg("user", "x".repeat(1000)), // 250 tok
      msg("assistant", "y".repeat(1000)), // 250 tok
      msg("user", "z".repeat(1000)), // 250 tok — newest
    ];
    // budget of 300 → can only fit the newest (250) + part of the second,
    // but the second is assistant and would lead, so it's stripped
    const out = capHistory(msgs, 300);
    expect(out).toHaveLength(1);
    expect(out[0].content[0]).toBe("z");
  });

  it("never starts with an assistant turn after trimming", () => {
    const msgs = [
      msg("user", "a"),
      msg("assistant", "b".repeat(1000)), // big
      msg("assistant", "c"), // would lead after trim
      msg("user", "d"), // newest
    ];
    const out = capHistory(msgs, 10); // tiny budget keeps only the newest
    expect(out[0].role).toBe("user");
    expect(out[out.length - 1].content).toBe("d");
  });

  it("does not mutate the input array", () => {
    const msgs = [
      msg("user", "a"),
      msg("assistant", "b"),
      msg("user", "c"),
    ];
    const snapshot = [...msgs];
    capHistory(msgs, 1);
    expect(msgs).toEqual(snapshot);
  });

  it("respects the real OpenRouter budget constant", () => {
    // a realistic thread that fits well within the 120k budget
    const msgs = Array.from({ length: 20 }, (_, i) =>
      msg(i % 2 === 0 ? "user" : "assistant", "x".repeat(500),
      ),
    );
    expect(capHistory(msgs, OPENROUTER_HISTORY_BUDGET)).toHaveLength(20);
  });
});
