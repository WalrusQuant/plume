import type { ChatMessage } from "$lib/api";
import type { AIProvider } from "./aiSettings";

// Upper bound (estimated tokens) on the chat history we send. The full thread
// always stays in storage and the UI — this only trims what goes over the wire.
/** OpenRouter / unknown custom servers: no server-side compaction, so this
    is a real cap. A long thread drops its oldest turns once it crosses. */
export const OPENROUTER_HISTORY_BUDGET = 120_000;
/** Grok 4.6 window is 500k; leave headroom for the system prompt + tools. */
export const GROK_HISTORY_BUDGET = 400_000;
/** GPT-5.6 windows are 1M+; same headroom idea as Grok. */
export const OPENAI_HISTORY_BUDGET = 400_000;
/** Anthropic uses server-side compaction (summarizes at ~150K input), so this
    is only a backstop set far above the trigger — it effectively never fires.
    Because compaction keeps the input near 150K, the newest turns (incl. the
    current compaction anchor) sit well inside this window in practice; the cap
    is a last-resort guard against a pathological >600K thread, not a context
    manager. */
export const ANTHROPIC_HISTORY_BUDGET = 600_000;

/** Wire-side history budget for the active connector. */
export function historyBudgetFor(provider: AIProvider): number {
  switch (provider) {
    case "anthropic":
      return ANTHROPIC_HISTORY_BUDGET;
    case "openai":
      return OPENAI_HISTORY_BUDGET;
    case "grok":
      return GROK_HISTORY_BUDGET;
    case "openrouter":
    case "custom":
      return OPENROUTER_HISTORY_BUDGET;
  }
}

/** Must match storage.rs::DEFAULT_CHAT_TITLE — signals an un-titled chat. */
export const DEFAULT_CHAT_TITLE = "New chat";

/** Derive a short chat title from the first user message. */
export function deriveTitle(text: string): string {
  const t = text.trim().replace(/\s+/g, " ");
  if (!t) return DEFAULT_CHAT_TITLE;
  // slice by code points so an emoji at the boundary isn't split in half
  const points = [...t];
  return points.length > 40 ? `${points.slice(0, 40).join("")}…` : t;
}

/** Build the API payload from the visible thread. Merges consecutive
    same-role messages (e.g. after a stop before the first token, or an
    errored turn) — the Anthropic API rejects non-alternating roles. A
    block-bearing turn (one carrying a compaction summary in `rawContent`)
    is never merged and always keeps its `rawContent`, so the summary
    round-trips verbatim. */
export function toApiMessages(messages: ChatMessage[]): ChatMessage[] {
  const out: ChatMessage[] = [];
  for (const m of messages) {
    const last = out[out.length - 1];
    // only merge when BOTH sides are plain text — never concat into or out of
    // a block-bearing turn (its content is a content-block array, not a string)
    if (last && last.role === m.role && !last.rawContent && !m.rawContent) {
      last.content += `\n\n${m.content}`;
    } else {
      out.push({ role: m.role, content: m.content, rawContent: m.rawContent });
    }
  }
  return out;
}

/** Rough token estimate for a message body (~4 chars/token). Only used to
    bound how much history we send — never billed, so an approximation is fine. */
export function estimateTokens(content: string): number {
  return Math.ceil(content.length / 4);
}

/** Trim the history so a long thread can't blow up cost or overflow the context
    window. Keeps the most recent turns within `budget` estimated tokens (the
    just-sent user message is always kept) and never starts the payload with an
    assistant turn — Anthropic requires the first message to be `user`, and
    OpenRouter is happier that way too. Operates on a copy; the stored thread
    and the UI are untouched. */
export function capHistory(messages: ChatMessage[], budget: number): ChatMessage[] {
  if (messages.length === 0) return messages;
  let total = estimateTokens(messages[messages.length - 1].content);
  let start = messages.length - 1; // always include the newest turn
  for (let i = messages.length - 2; i >= 0; i--) {
    const t = estimateTokens(messages[i].content);
    if (total + t > budget) break;
    total += t;
    start = i;
  }
  // dropping older turns can leave a leading assistant message — strip those
  while (start < messages.length - 1 && messages[start].role === "assistant") start++;
  return messages.slice(start);
}
