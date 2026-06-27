import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, type AIProvider, type Chat, type ChatMessage, type DocReference } from "$lib/api";
import { toast } from "$lib/toast.svelte";
import { formatError } from "$lib/formatError";

const SETTINGS_KEY = "markdown-ai-settings";
/** Must match storage.rs::DEFAULT_CHAT_TITLE — signals an un-titled chat. */
const DEFAULT_CHAT_TITLE = "New chat";

// Upper bound (estimated tokens) on the chat history we send. The full thread
// always stays in storage and the UI — this only trims what goes over the wire.
/** OpenRouter has no server-side compaction, so this is a real cap: a long
    thread drops its oldest turns once it crosses the budget. */
const OPENROUTER_HISTORY_BUDGET = 120_000;
/** Anthropic uses server-side compaction (summarizes at ~150K input), so this
    is only a backstop set far above the trigger — it effectively never fires.
    Because compaction keeps the input near 150K, the newest turns (incl. the
    current compaction anchor) sit well inside this window in practice; the cap
    is a last-resort guard against a pathological >600K thread, not a context
    manager. */
const ANTHROPIC_HISTORY_BUDGET = 600_000;

export const DEFAULT_MODELS: Record<AIProvider, string> = {
  anthropic: "claude-opus-4-8",
  openrouter: "anthropic/claude-opus-4.8",
};

export interface AISettings {
  provider: AIProvider;
  model: string;
  /** Global "Voice & tone" guidance injected into every AI system prompt. */
  voice: string;
  /** When on, chat may call the Tavily web_search tool (requires a Tavily key). */
  webSearch: boolean;
}

function loadSettings(): AISettings {
  const defaults: AISettings = {
    provider: "anthropic",
    model: DEFAULT_MODELS.anthropic,
    voice: "",
    webSearch: false,
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
function toApiMessages(messages: ChatMessage[]): ChatMessage[] {
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
function estimateTokens(content: string): number {
  return Math.ceil(content.length / 4);
}

/** Trim the history so a long thread can't blow up cost or overflow the context
    window. Keeps the most recent turns within `budget` estimated tokens (the
    just-sent user message is always kept) and never starts the payload with an
    assistant turn — Anthropic requires the first message to be `user`, and
    OpenRouter is happier that way too. Operates on a copy; the stored thread
    and the UI are untouched. */
function capHistory(messages: ChatMessage[], budget: number): ChatMessage[] {
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

/** Payloads of the assistant:* events emitted by ai.rs. */
interface StreamToken {
  id: string;
  text: string;
}
interface StreamDone {
  id: string;
  /** True when the stream was aborted (superseded by another AI action) —
      the accumulated text is truncated, not a completed response. */
  aborted?: boolean;
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
interface StreamContent {
  id: string;
  /** Raw assistant content-block array (incl. a compaction block) to persist
      and replay verbatim. Emitted only when the turn produced a compaction. */
  content: unknown;
}
interface StreamStatus {
  id: string;
  /** Transient activity line (e.g. "Searching the web…") shown between tokens
      while a tool call runs; cleared on the next token / done / stop. */
  message: string;
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
  /** Whether a Tavily key is saved (web search can actually run). */
  hasTavilyKey = $state(false);
  /** Transient tool-activity line shown during a turn (e.g. "Searching…"). */
  searchStatus = $state<string | null>(null);
  /** True when the last send dropped older turns to fit the OpenRouter cap
      (which, unlike Anthropic, has no server-side compaction). */
  historyTrimmed = $state(false);
  settings = $state<AISettings>(loadSettings());

  /** The context limit the UI should warn against, or null when there's no
      hard one (Anthropic relies on server-side compaction). */
  get contextLimit(): number | null {
    return this.settings.provider === "openrouter" ? OPENROUTER_HISTORY_BUDGET : null;
  }

  private docId: string | null = null;
  /** Id of the in-flight stream; events with any other id are stale. */
  private activeStreamId: string | null = null;
  /** Set by the error handler so the following `done` knows the turn failed
      and must drop its truncated partial reply. */
  private streamErrored = false;
  private unlisteners: UnlistenFn[] = [];
  private listening = false;
  /** Idle watchdog: cleared on each token; fires a timeout if a stream stalls. */
  private idleTimer: ReturnType<typeof setTimeout> | null = null;
  /** Max idle time with no token/status/error/done before a stream is treated as hung. */
  private static readonly IDLE_TIMEOUT_MS = 90_000;

  private resetIdleTimer() {
    if (this.idleTimer) clearTimeout(this.idleTimer);
    this.idleTimer = setTimeout(() => {
      // No activity arrived in time — treat as a hung connection.
      toast.error("The reply timed out — no response from the provider.");
      void this.stop();
    }, AssistantStore.IDLE_TIMEOUT_MS);
  }

  private clearIdleTimer() {
    if (this.idleTimer) {
      clearTimeout(this.idleTimer);
      this.idleTimer = null;
    }
  }

  get activeChat(): Chat | undefined {
    return this.chats.find((c) => c.id === this.activeChatId);
  }

  async init() {
    this.isConfigured = await api.hasApiKey(this.settings.provider);
    this.hasTavilyKey = await api.hasTavilyKey();
    if (this.listening) return; // re-init (e.g. remount): refresh key status only
    this.listening = true;
    this.unlisteners = await Promise.all([
      listen<StreamToken>("assistant:token", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        this.searchStatus = null; // first reply token ends the tool-activity line
        this.resetIdleTimer();
        this.appendToken(e.payload.text);
      }),
      listen<StreamStatus>("assistant:status", (e) => {
        if (e.payload.id === this.activeStreamId) {
          this.searchStatus = e.payload.message;
          this.resetIdleTimer(); // tool activity (e.g. web search) is progress
        }
      }),
      listen<StreamUsage>("assistant:usage", (e) => {
        if (e.payload.id === this.activeStreamId) this.recordUsage(e.payload);
      }),
      listen<StreamContent>("assistant:content", (e) => {
        // a compaction block was produced — stash the raw content-block array on
        // the last assistant message so it persists and replays verbatim. Emitted
        // before `done`, so the done-handler's persist() captures it.
        if (e.payload.id === this.activeStreamId) this.recordContent(e.payload.content);
      }),
      listen<StreamDone>("assistant:done", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        this.clearIdleTimer();
        this.activeStreamId = null;
        this.isStreaming = false;
        this.searchStatus = null;
        const errored = this.streamErrored;
        this.streamErrored = false;
        // The reply did NOT complete on its own — it was either superseded by
        // another AI action (inline edit / expand / multiply took the single
        // stream slot → `aborted`) or it errored mid-stream. Either way the
        // accumulated text is a truncated fragment: drop it so it is never
        // shown, persisted, or replayed to the model as a genuine turn.
        if (e.payload.aborted || errored) {
          this.dropTrailingAssistant();
          if (e.payload.aborted) {
            toast.error("Chat reply was interrupted by another AI action.");
          }
        }
        void this.persist();
      }),
      listen<StreamError>("assistant:error", (e) => {
        // a matching `done` event follows and finishes the stream; flag the
        // failure so that handler drops the partial. Surface via toast — an
        // "Error: …" pseudo-message would be persisted and replayed to the
        // model as a genuine assistant turn.
        if (e.payload.id !== this.activeStreamId) return;
        this.streamErrored = true;
        toast.error(`Assistant error: ${e.payload.message}`);
      }),
    ]);
  }

  /** Monotonic guard for loadFor: rapid doc switches interleave fetches, and
      without it the slower fetch can win and show the wrong doc's chats. */
  private loadSeq = 0;

  /** Switch to a document: load its chats and open the most recent one. */
  async loadFor(docId: string | null) {
    if (docId === this.docId) return;
    const seq = ++this.loadSeq;
    if (this.isStreaming) await this.stop();
    await this.persist();
    if (seq !== this.loadSeq) return; // superseded by a newer switch
    this.docId = docId;
    if (!docId) {
      this.chats = [];
      this.activeChatId = null;
      this.messages = [];
      return;
    }
    try {
      let chats = await api.listChats(docId);
      if (chats.length === 0) {
        chats = [await api.createChat(docId)];
      }
      if (seq !== this.loadSeq) return;
      const messages = await api.getChatMessages(chats[0].id);
      if (seq !== this.loadSeq) return;
      this.chats = chats;
      this.activeChatId = chats[0].id;
      this.messages = messages;
    } catch (e) {
      toast.error(`Couldn't load chat: ${formatError(e)}`);
      this.chats = [];
      this.activeChatId = null;
      this.messages = [];
    }
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

  /** Remove a trailing partial assistant message (after an aborted/errored
      stream) so the truncated text isn't kept or replayed to the model. */
  private dropTrailingAssistant() {
    if (this.messages[this.messages.length - 1]?.role === "assistant") {
      this.messages = this.messages.slice(0, -1);
    }
  }

  private recordUsage(u: StreamUsage) {
    const last = this.messages[this.messages.length - 1];
    if (last?.role === "assistant") {
      last.inputTokens = u.inputTokens;
      last.outputTokens = u.outputTokens;
    }
  }

  private recordContent(content: unknown) {
    // ensure an assistant message exists: a turn that produced a compaction
    // block but no visible text would have created none (the message is created
    // lazily on the first token), and dropping rawContent here would lose the
    // summary and leave replay without its anchor
    let last = this.messages[this.messages.length - 1];
    if (last?.role !== "assistant") {
      this.messages = [...this.messages, { role: "assistant", content: "" }];
      last = this.messages[this.messages.length - 1];
    }
    last.rawContent = content;
  }

  /** History budget for the active provider — Anthropic is a high backstop
      (server-side compaction does the real work); OpenRouter is a hard cap. */
  private historyBudget(): number {
    return this.settings.provider === "anthropic"
      ? ANTHROPIC_HISTORY_BUDGET
      : OPENROUTER_HISTORY_BUDGET;
  }

  /** Returns true once the request is accepted (stream started); false if it
      was rejected up front or the send failed — the caller can then restore the
      user's input instead of making them retype it. */
  async send(
    userMessage: string,
    documentContent: string,
    references: DocReference[] = [],
  ): Promise<boolean> {
    if (this.isStreaming || !this.activeChatId) return false;
    const chat = this.activeChat;
    this.messages = [...this.messages, { role: "user", content: userMessage }];
    this.isStreaming = true;
    this.streamErrored = false;
    this.activeStreamId = crypto.randomUUID();
    // cap the sent history so a long thread can't blow up cost / overflow.
    // Anthropic relies on server-side compaction (high backstop); OpenRouter
    // has no compaction, so its cap is the real limiter — flag when it bites.
    const snapshot = $state.snapshot(this.messages);
    const capped = capHistory(snapshot, this.historyBudget());
    this.historyTrimmed = this.settings.provider === "openrouter" && capped.length < snapshot.length;
    try {
      await api.sendAssistantMessage(
        this.activeStreamId,
        this.settings.provider,
        this.settings.model || null,
        toApiMessages(capped),
        documentContent,
        references,
        this.settings.webSearch,
        this.settings.voice || null,
      );
      this.resetIdleTimer();
      // the request was accepted (key present, stream started) — only now
      // auto-title a still-default chat from its first message, so a failed
      // send doesn't rename a chat for an exchange that never happened
      if (chat && chat.title === DEFAULT_CHAT_TITLE) {
        void this.renameChat(chat.id, deriveTitle(userMessage));
      }
      return true;
    } catch (e) {
      toast.error(`Sending message failed: ${formatError(e)}`);
      this.clearIdleTimer();
      this.isStreaming = false;
      this.activeStreamId = null;
      // roll back the optimistic user message so it isn't persisted or merged
      // into the next send's payload
      const last = this.messages[this.messages.length - 1];
      if (last?.role === "user" && last.content === userMessage) {
        this.messages = this.messages.slice(0, -1);
      }
      return false;
    }
  }

  async stop() {
    this.clearIdleTimer();
    // drop the id first so events still in flight are ignored
    this.activeStreamId = null;
    this.isStreaming = false;
    this.streamErrored = false;
    this.searchStatus = null;
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

  async saveTavilyKey(key: string) {
    await api.setTavilyKey(key);
    this.hasTavilyKey = true;
  }

  async removeTavilyKey() {
    await api.deleteTavilyKey();
    this.hasTavilyKey = false;
  }

  /** Toggle web search. Turning it on without a Tavily key is rejected (the
      caller should surface the reason and open Settings) so the toggle never
      sits "on" while every search would fail. Returns whether it is now on. */
  async toggleWebSearch(): Promise<boolean> {
    const next = !this.settings.webSearch;
    if (next && !this.hasTavilyKey) return false;
    await this.updateSettings({ ...this.settings, webSearch: next });
    return next;
  }
}

export const assistant = new AssistantStore();
