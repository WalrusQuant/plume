import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, type AIProvider, type ChatMessage } from "$lib/api";

const SETTINGS_KEY = "markdown-ai-settings";

export const DEFAULT_MODELS: Record<AIProvider, string> = {
  anthropic: "claude-opus-4-8",
  openrouter: "anthropic/claude-opus-4.8",
};

export interface AISettings {
  provider: AIProvider;
  model: string;
}

function loadSettings(): AISettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) return JSON.parse(raw);
  } catch {
    /* fall through to defaults */
  }
  return { provider: "anthropic", model: DEFAULT_MODELS.anthropic };
}

/** Chat state + Tauri event plumbing for the AI panel. The HTTP call and the
    API key live in Rust; this store only sends commands and accumulates the
    streamed tokens. Provider/model (not secret) persist in localStorage. */
class AssistantStore {
  messages = $state<ChatMessage[]>([]);
  isStreaming = $state(false);
  isConfigured = $state(false);
  settings = $state<AISettings>(loadSettings());

  private docId: string | null = null;
  private unlisteners: UnlistenFn[] = [];

  async init() {
    this.isConfigured = await api.hasApiKey(this.settings.provider);
    this.unlisteners = await Promise.all([
      listen<string>("assistant:token", (e) => this.appendToken(e.payload)),
      listen("assistant:done", () => {
        this.isStreaming = false;
        void this.persist();
      }),
      listen<string>("assistant:error", (e) => {
        this.messages = [...this.messages, { role: "assistant", content: `Error: ${e.payload}` }];
        this.isStreaming = false;
        void this.persist();
      }),
    ]);
  }

  /** Switch the chat thread to a document (loads its saved history). */
  async loadFor(docId: string | null) {
    if (docId === this.docId) return;
    if (this.isStreaming) await this.stop();
    await this.persist();
    this.docId = docId;
    this.messages = docId ? await api.getChatMessages(docId) : [];
  }

  private async persist() {
    if (this.docId) {
      await api.saveChatMessages(this.docId, $state.snapshot(this.messages));
    }
  }

  destroy() {
    this.unlisteners.forEach((fn) => fn());
    this.unlisteners = [];
  }

  private appendToken(text: string) {
    const last = this.messages[this.messages.length - 1];
    if (last?.role === "assistant") {
      last.content += text;
    } else {
      this.messages = [...this.messages, { role: "assistant", content: text }];
    }
  }

  async send(userMessage: string, documentContent: string) {
    if (this.isStreaming) return;
    this.messages = [...this.messages, { role: "user", content: userMessage }];
    this.isStreaming = true;
    try {
      await api.sendAssistantMessage(
        this.settings.provider,
        this.settings.model || null,
        $state.snapshot(this.messages),
        documentContent,
      );
    } catch (e) {
      this.messages = [...this.messages, { role: "assistant", content: `Error: ${e}` }];
      this.isStreaming = false;
    }
  }

  async stop() {
    await api.stopAssistant();
  }

  clear() {
    this.messages = [];
    void this.persist();
  }

  async updateSettings(settings: AISettings) {
    this.settings = settings;
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    this.isConfigured = await api.hasApiKey(settings.provider);
  }

  async saveKey(key: string) {
    await api.setApiKey(this.settings.provider, key);
    this.isConfigured = true;
  }

  async removeKey() {
    await api.deleteApiKey(this.settings.provider);
    this.isConfigured = false;
  }
}

export const assistant = new AssistantStore();
