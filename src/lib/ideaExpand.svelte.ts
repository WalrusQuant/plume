import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "$lib/api";
import { assistant } from "$lib/assistant.svelte";

// ---------------------------------------------------------------------------
// Idea expansion — turn a captured idea fragment into a full draft of a chosen
// document type. Headless: no editor, no chat. Reuses the chat `assistant:*`
// events (filtered by stream id) and the single AiState slot, so starting an
// expansion aborts any in-flight chat/inline stream and vice versa — same
// mutual-exclusion model as inline edit. See ai.rs::start_expand_stream.
//
// `expand()` resolves with the full generated draft once the stream completes,
// so the caller can create + open a new document from it.
// ---------------------------------------------------------------------------

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

class IdeaExpandController {
  /** True while a draft is being generated — drives the Inbox spinner. */
  isExpanding = $state(false);

  /** Id of the in-flight expand stream; events with any other id are stale. */
  private activeStreamId: string | null = null;
  private streamed = "";
  private resolve: ((text: string) => void) | null = null;
  private reject: ((err: Error) => void) | null = null;
  private unlisteners: UnlistenFn[] = [];
  private listening = false;

  /** Register the Tauri event listeners once. Mirrors AssistantStore.init. */
  async init() {
    if (this.listening) return;
    this.listening = true;
    this.unlisteners = await Promise.all([
      listen<StreamToken>("assistant:token", (e) => {
        if (e.payload.id === this.activeStreamId) this.streamed += e.payload.text;
      }),
      listen<StreamDone>("assistant:done", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        this.finish(this.streamed.trim());
      }),
      listen<StreamError>("assistant:error", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        this.fail(new Error(e.payload.message));
      }),
    ]);
  }

  destroy() {
    this.unlisteners.forEach((fn) => fn());
    this.unlisteners = [];
    this.listening = false;
  }

  /** Expand `idea` into a `targetLabel` draft. Resolves with the draft markdown
      when the stream completes; rejects on error. Only one expansion at a time. */
  expand(idea: string, targetLabel: string): Promise<string> {
    if (this.isExpanding) {
      return Promise.reject(new Error("an expansion is already running"));
    }
    if (!assistant.isConfigured) {
      return Promise.reject(new Error("Add an AI API key in Settings to expand ideas."));
    }
    this.streamed = "";
    this.isExpanding = true;
    this.activeStreamId = crypto.randomUUID();
    const promise = new Promise<string>((resolve, reject) => {
      this.resolve = resolve;
      this.reject = reject;
    });
    // model: null → backend uses the provider's default (strong) model
    api
      .sendIdeaExpand(this.activeStreamId, assistant.settings.provider, null, idea, targetLabel)
      .catch((e) => this.fail(e instanceof Error ? e : new Error(String(e))));
    return promise;
  }

  private finish(text: string) {
    const resolve = this.resolve;
    this.cleanup();
    resolve?.(text);
  }

  private fail(err: Error) {
    const reject = this.reject;
    this.cleanup();
    reject?.(err);
  }

  private cleanup() {
    this.activeStreamId = null;
    this.streamed = "";
    this.resolve = null;
    this.reject = null;
    this.isExpanding = false;
  }
}

export const ideaExpand = new IdeaExpandController();
