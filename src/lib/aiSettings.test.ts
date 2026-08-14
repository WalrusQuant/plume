import { describe, it, expect } from "vitest";
import { defaultSettings, mergeSettings, DEFAULT_MODELS } from "./aiSettings";

describe("mergeSettings", () => {
  it("returns defaults for missing or junk input", () => {
    expect(mergeSettings(null)).toEqual(defaultSettings());
    expect(mergeSettings("nope")).toEqual(defaultSettings());
  });

  it("keeps an older blob that has no custom fields", () => {
    const merged = mergeSettings({
      provider: "openrouter",
      model: "anthropic/claude-sonnet-4.6",
      voice: "plain",
      webSearch: true,
    });
    expect(merged.provider).toBe("openrouter");
    expect(merged.model).toBe("anthropic/claude-sonnet-4.6");
    expect(merged.voice).toBe("plain");
    expect(merged.webSearch).toBe(true);
    expect(merged.searchNotes).toBe(false);
    expect(merged.customConnectors).toEqual([]);
    expect(merged.customId).toBeNull();
  });

  it("falls back to Anthropic for an unknown provider", () => {
    expect(mergeSettings({ provider: "azure" }).provider).toBe("anthropic");
  });

  it("drops a customId that does not match a saved endpoint", () => {
    const merged = mergeSettings({
      provider: "custom",
      customId: "ghost",
      customConnectors: [{ id: "real", name: "Ollama", baseUrl: "http://localhost:11434/v1", model: "llama3" }],
    });
    expect(merged.customId).toBe("real");
  });

  it("preserves named custom endpoints", () => {
    const connectors = [
      { id: "a", name: "Ollama", baseUrl: "http://localhost:11434/v1", model: "llama3" },
      { id: "b", name: "Work", baseUrl: "https://proxy.example/v1", model: "gpt-4" },
    ];
    const merged = mergeSettings({
      provider: "custom",
      customId: "b",
      customConnectors: connectors,
      model: "gpt-4",
    });
    expect(merged.customConnectors).toEqual(connectors);
    expect(merged.customId).toBe("b");
  });
});

describe("DEFAULT_MODELS", () => {
  it("has a default for every built-in", () => {
    expect(DEFAULT_MODELS.anthropic).toBe("claude-opus-4-8");
    expect(DEFAULT_MODELS.openai).toBe("gpt-5.6-sol");
    expect(DEFAULT_MODELS.grok).toBe("grok-4.6");
    expect(DEFAULT_MODELS.openrouter).toContain("claude");
  });
});
