import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, type AIProvider, type Chat, type ChatMessage } from "$lib/api";

const SETTINGS_KEY = "markdown-ai-settings";
/** Must match storage.rs::DEFAULT_CHAT_TITLE — signals an un-titled chat. */
const DEFAULT_CHAT_TITLE = "New chat";

export const DEFAULT_MODELS: Record<AIProvider, string> = {
  anthropic: "claude-opus-4-8",
  openrouter: "anthropic/claude-opus-4.8",
};

export interface AISettings {
  provider: AIProvider;
  model: string;
  /** Global "Voice & tone" guidance injected into every AI system prompt. */
  voice: string;
}

function loadSettings(): AISettings {
  const defaults: AISettings = {
    provider: "anthropic",
    model: DEFAULT_MODELS.anthropic,
    voice: "",
  };
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    // merge so blobs saved before `voice` existed still get a defined value
    if (raw) return { ...defaults, ...JSON.parse(raw) };
  } catch {
    /* fall through to defaults */
  }
  return defaults;
}

/** Derive a short chat title from the first user message. */
function deriveTitle(text: string): string {
  const t = text.trim().replace(/\s+/g, " ");
  if (!t) return DEFAULT_CHAT_TITLE;
  return t.length > 40 ? `${t.slice(0, 40)}…` : t;
}

/** Payloads of the assistant:* events emitted by ai.rs. */
interface StreamToken {
  id: string;
  text: string;
}
interface StreamDone {
  id: string;
}
interface StreamError {
  id: string;
  message: string;
}
interface StreamUsage {
  id: string;
  inputTokens: number;
  outputTokens: number;
}

/** Chat state + Tauri event plumbing for the AI panel. Each document has one
    or more chat threads; the HTTP call and the API key live in Rust, this
    store sends commands, accumulates streamed tokens, and tracks token usage.
    Provider/model (not secret) persist in localStorage. */
class AssistantStore {
  messages = $state<ChatMessage[]>([]);
  chats = $state<Chat[]>([]);
  activeChatId = $state<string | null>(null);
  isStreaming = $state(false);
  isConfigured = $state(false);
  settings = $state<AISettings>(loadSettings());

  private docId: string | null = null;
  /** Id of the in-flight stream; events with any other id are stale. */
  private activeStreamId: string | null = null;
  private unlisteners: UnlistenFn[] = [];
  private listening = false;

  get activeChat(): Chat | undefined {
    return this.chats.find((c) => c.id === this.activeChatId);
  }

  async init() {
    this.isConfigured = await api.hasApiKey(this.settings.provider);
    if (this.listening) return; // re-init (e.g. remount): refresh key status only
    this.listening = true;
    this.unlisteners = await Promise.all([
      listen<StreamToken>("assistant:token", (e) => {
        if (e.payload.id === this.activeStreamId) this.appendToken(e.payload.text);
      }),
      listen<StreamUsage>("assistant:usage", (e) => {
        if (e.payload.id === this.activeStreamId) this.recordUsage(e.payload);
      }),
      listen<StreamDone>("assistant:done", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        this.activeStreamId = null;
        this.isStreaming = false;
        void this.persist();
      }),
      listen<StreamError>("assistant:error", (e) => {
        // a matching done event follows and finishes the stream
        if (e.payload.id !== this.activeStreamId) return;
        this.messages = [
          ...this.messages,
          { role: "assistant", content: `Error: ${e.payload.message}` },
        ];
      }),
    ]);
  }

  /** Switch to a document: load its chats and open the most recent one. */
  async loadFor(docId: string | null) {
    if (docId === this.docId) return;
    if (this.isStreaming) await this.stop();
    await this.persist();
    this.docId = docId;
    if (!docId) {
      this.chats = [];
      this.activeChatId = null;
      this.messages = [];
      return;
    }
    this.chats = await api.listChats(docId);
    if (this.chats.length === 0) {
      this.chats = [await api.createChat(docId)];
    }
    this.activeChatId = this.chats[0].id;
    this.messages = await api.getChatMessages(this.activeChatId);
  }

  async selectChat(chatId: string) {
    if (chatId === this.activeChatId) return;
    if (this.isStreaming) await this.stop();
    await this.persist();
    this.activeChatId = chatId;
    this.messages = await api.getChatMessages(chatId);
  }

  async newChat() {
    if (!this.docId) return;
    if (this.isStreaming) await this.stop();
    await this.persist();
    const chat = await api.createChat(this.docId);
    this.chats = [chat, ...this.chats];
    this.activeChatId = chat.id;
    this.messages = [];
  }

  async renameChat(chatId: string, title: string) {
    const updated = await api.renameChat(chatId, title);
    this.chats = this.chats.map((c) => (c.id === chatId ? updated : c));
  }

  async deleteChat(chatId: string) {
    await api.deleteChat(chatId);
    this.chats = this.chats.filter((c) => c.id !== chatId);
    if (this.activeChatId === chatId) {
      if (this.chats.length === 0 && this.docId) {
        this.chats = [await api.createChat(this.docId)];
      }
      this.activeChatId = this.chats[0]?.id ?? null;
      this.messages = this.activeChatId ? await api.getChatMessages(this.activeChatId) : [];
    }
  }

  private async persist() {
    if (this.activeChatId) {
      await api.saveChatMessages(this.activeChatId, $state.snapshot(this.messages));
    }
  }

  destroy() {
    this.unlisteners.forEach((fn) => fn());
    this.unlisteners = [];
    this.listening = false;
  }

  private appendToken(text: string) {
    const last = this.messages[this.messages.length - 1];
    if (last?.role === "assistant") {
      last.content += text;
    } else {
      this.messages = [...this.messages, { role: "assistant", content: text }];
    }
  }

  private recordUsage(u: StreamUsage) {
    const last = this.messages[this.messages.length - 1];
    if (last?.role === "assistant") {
      last.inputTokens = u.inputTokens;
      last.outputTokens = u.outputTokens;
    }
  }

  async send(userMessage: string, documentContent: string) {
    if (this.isStreaming || !this.activeChatId) return;
    // auto-title a still-default chat from its first message
    const chat = this.activeChat;
    if (chat && chat.title === DEFAULT_CHAT_TITLE) {
      void this.renameChat(chat.id, deriveTitle(userMessage));
    }
    this.messages = [...this.messages, { role: "user", content: userMessage }];
    this.isStreaming = true;
    this.activeStreamId = crypto.randomUUID();
    try {
      await api.sendAssistantMessage(
        this.activeStreamId,
        this.settings.provider,
        this.settings.model || null,
        $state.snapshot(this.messages),
        documentContent,
        this.settings.voice || null,
      );
    } catch (e) {
      this.messages = [...this.messages, { role: "assistant", content: `Error: ${e}` }];
      this.isStreaming = false;
      this.activeStreamId = null;
    }
  }

  async stop() {
    // drop the id first so events still in flight are ignored
    this.activeStreamId = null;
    this.isStreaming = false;
    await api.stopAssistant();
    await this.persist();
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
