export type AIProvider = "anthropic" | "openai" | "grok" | "openrouter" | "custom";

export const BUILTIN_PROVIDERS = ["anthropic", "openai", "grok", "openrouter"] as const;
export type BuiltinProvider = (typeof BUILTIN_PROVIDERS)[number];

export interface CustomConnector {
  id: string;
  name: string;
  baseUrl: string;
  model: string;
}

export interface AISettings {
  provider: AIProvider;
  /** Which named custom endpoint is active when `provider === "custom"`. */
  customId: string | null;
  customConnectors: CustomConnector[];
  model: string;
  /** Global "Voice & tone" guidance injected into every AI system prompt. */
  voice: string;
  webSearch: boolean;
  searchNotes: boolean;
}

export const DEFAULT_MODELS: Record<BuiltinProvider, string> = {
  anthropic: "claude-opus-4-8",
  openai: "gpt-5.6-sol",
  grok: "grok-4.6",
  openrouter: "anthropic/claude-opus-4.8",
};

const VALID_PROVIDERS: AIProvider[] = [
  "anthropic",
  "openai",
  "grok",
  "openrouter",
  "custom",
];

export function defaultSettings(): AISettings {
  return {
    provider: "anthropic",
    customId: null,
    customConnectors: [],
    model: DEFAULT_MODELS.anthropic,
    voice: "",
    webSearch: false,
    searchNotes: false,
  };
}

export function isBuiltinProvider(value: string): value is BuiltinProvider {
  return (BUILTIN_PROVIDERS as readonly string[]).includes(value);
}

/** Merge a stored settings blob with defaults so older localStorage still loads. */
export function mergeSettings(raw: unknown): AISettings {
  const defaults = defaultSettings();
  if (!raw || typeof raw !== "object") return defaults;
  const parsed = raw as Partial<AISettings> & { provider?: string };
  const provider = VALID_PROVIDERS.includes(parsed.provider as AIProvider)
    ? (parsed.provider as AIProvider)
    : defaults.provider;
  const customConnectors = Array.isArray(parsed.customConnectors)
    ? parsed.customConnectors.filter(isCustomConnector)
    : [];
  const customId =
    typeof parsed.customId === "string" && customConnectors.some((c) => c.id === parsed.customId)
      ? parsed.customId
      : provider === "custom" && customConnectors[0]
        ? customConnectors[0].id
        : null;
  return {
    ...defaults,
    ...parsed,
    provider,
    customId,
    customConnectors,
    model: typeof parsed.model === "string" ? parsed.model : defaults.model,
    voice: typeof parsed.voice === "string" ? parsed.voice : defaults.voice,
    webSearch: Boolean(parsed.webSearch),
    searchNotes: Boolean(parsed.searchNotes),
  };
}

function isCustomConnector(value: unknown): value is CustomConnector {
  if (!value || typeof value !== "object") return false;
  const c = value as CustomConnector;
  return (
    typeof c.id === "string" &&
    c.id.trim().length > 0 &&
    typeof c.name === "string" &&
    typeof c.baseUrl === "string" &&
    typeof c.model === "string"
  );
}

export function defaultModelFor(settings: AISettings): string {
  if (settings.provider === "custom") {
    const c = settings.customConnectors.find((x) => x.id === settings.customId);
    return c?.model ?? "";
  }
  return DEFAULT_MODELS[settings.provider];
}
